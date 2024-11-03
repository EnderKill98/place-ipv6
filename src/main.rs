//! https://v6staging.sys42.net/
//! https://github.com/Xevion/v6-place

use std::{
    collections::VecDeque,
    io::{stdin, Read},
    net::Ipv6Addr,
    path::PathBuf,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread,
    time::Instant,
};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{bail, eyre, Context};
use color_eyre::Result;
use mac_address::MacAddress;
use place_ipv6::*;
use rand::seq::SliceRandom;

#[macro_use]
extern crate log;

#[derive(Subcommand, Clone)]
enum Commands {
    /// Read a stream of raw rgb or rgba frames in 1920x1080 size (usually from ffmpeg with the flags `-pix_fmt rgb24/rgba -f rawvideo pipe:1`)
    RawPipeStdin {
        /// Stop sending a pixel if it was the same for given amound of processed/sent frames (0 = send always regardless). HIGH NUMBERS CAN FILL YOUR RAM OVER TIME!
        #[arg(short = 'r', long, default_value = "0")]
        resend_same_pixel_max: usize,

        /// Width of input framebuffers
        width: u16,

        /// Height of input framebuffers
        height: u16,

        /// Expect an additional alpha channel at the end (turning rgb(24) into rgba)
        #[arg(short = 'a', long, action)]
        has_alpha: bool,
    },
    Image {
        /// Path to image that should be displayed (you can use "-" for stdin)
        path: PathBuf,

        /// At what alpha value a pixel should be sent (if image has an alpha channel). If missing, alpha will be ignored.
        #[arg(short = 'a', long)]
        alpha_threshold: Option<u8>,

        /// If set, will continously loop, sending the image
        #[arg(short = 'c', long, action)]
        continuous: bool,
    },
}

#[derive(Parser, Clone)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// What interface to send the packets on.
    #[arg(short = 'i', long)]
    iface_name: String,
    /// The IPv6 assigned to the specified interface.
    #[arg(short = 's', long)]
    src_ip: Ipv6Addr,
    /// The Mac of the next hop where the packet needs to go to/through.
    /// Use e.g. `ip route get 2620:119:35::35` and `ip -6 neigh` to find it.
    #[arg(short = 'd', long)]
    dest_mac: MacAddress,
    /// If set, limit transmission speed to the given packets/sec.
    #[arg(short = 'r', long)]
    packets_per_sec: Option<u32>,
    /// Send pings in random order. Updates will appear like noise.
    #[arg(short = 'n', long, action)]
    noisy: bool,

    /// Offset image by this
    #[arg(short = 'x', long, default_value = "0")]
    offset_x: u16,
    /// Offset image by this
    #[arg(short = 'y', long, default_value = "0")]
    offset_y: u16,

    /// Skip all pixels smaller than given value (at input resolution)
    #[arg(long, default_value = "0")]
    min_x: u16,
    /// Skip all pixels bigger than given value (at input resolution)
    #[arg(long, default_value = "9999")]
    max_x: u16,

    /// Skip all pixels smaller than given value (at input resolution)
    #[arg(long, default_value = "0")]
    min_y: u16,
    /// Skip all pixels bigger than given value (at input resolution)
    #[arg(long, default_value = "9999")]
    max_y: u16,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }

    env_logger::builder().format_timestamp_millis().init();

    match args.command {
        Commands::RawPipeStdin {
            resend_same_pixel_max,
            width,
            height,
            has_alpha,
        } => run_rawpipe_stdin(args.clone(), resend_same_pixel_max, width, height, has_alpha),
        Commands::Image {
            ref path,
            alpha_threshold,
            continuous,
        } => run_image(
            args.clone(),
            path.clone(),
            alpha_threshold,
            continuous,
        ),
    }
}

fn run_rawpipe_stdin(args: Args, resend_same_pixel_max: usize, width: u16, height: u16, has_alpha: bool) -> Result<()> {
    let bytes_per_pixel = if has_alpha { 4 } else { 3 };
    let bytes_per_frame: usize = ((width as u32) * (height as u32) * bytes_per_pixel) as usize;

    if args.offset_x + width > 1920 {
        bail!("Can't send framebuffer! X Offset + Width could cause the framebuffer to get out-of-bounds!");
    }

    if args.offset_y + height > 1080 {
        bail!("Can't send framebuffer! Y Offset + Height could cause the framebuffer to get out-of-bounds!");
    }

    let (tx, rx): (
        SyncSender<Vec<u8>>,
        Receiver<Vec<u8>>,
    ) = sync_channel(1); // 1 Buffered frame
    let thread_handle = thread::spawn(move || {
        let mut rng: rand::rngs::ThreadRng = rand::thread_rng();

        // Ready
        let mut counter: u64 = 0;
        let iface_name = &args.iface_name;
        let lib = rawsock::open_best_library().unwrap();
        let iface = lib.open_interface(iface_name).unwrap();
        iface.break_loop();

        let src_mac = mac_address::mac_address_by_name(iface_name)
            .unwrap()
            .ok_or(eyre!("No mac :("))
            .unwrap();
        let dest_mac = args.dest_mac;
        info!("RX: Src Mac: {}", src_mac);
        let ethernet_info = match iface.data_link() {
            rawsock::DataLink::Ethernet => Some(EthernetInfo::new(src_mac, dest_mac)),
            _ => None,
        };

        let mut packet_counter;
        info!("RX: Ready...");
        let src_ip = args.src_ip;
        let mut last_sec = Instant::now();
        let mut last_sec_counter = 0;

        let mut last_frames: VecDeque<Vec<u8>> =
            VecDeque::with_capacity(resend_same_pixel_max);
        let color_at = |buf: &[u8], x: u16, y: u16| {
            let index = bytes_per_pixel as usize * width as usize * y as usize + bytes_per_pixel as usize * x as usize;
            if bytes_per_pixel == 3 {
                Color::new(buf[index], buf[index + 1], buf[index + 2])
            }else {
                Color::new_alpha(buf[index], buf[index + 1], buf[index + 2], buf[index + 3])
            }
        };

        for buffer in rx {
            let mut x = 0;
            let mut y = 0;
            let started_at = Instant::now();
            packet_counter = 0;

            let mut data_array = Vec::new();

            info!("RX: Processing frame...");
            for buffer_index in (0..buffer.len()).step_by(bytes_per_pixel as usize) {
                let color = Color::new_alpha(
                    buffer[buffer_index],
                    buffer[buffer_index + 1],
                    buffer[buffer_index + 2],
                    if bytes_per_pixel == 3 { 0xFF } else { buffer[buffer_index + 3] },
                );

                let mut send = true;

                if x < args.min_x || x > args.max_x || y < args.min_y || y > args.max_y {
                    send = false;
                }

                if send && resend_same_pixel_max > 0 && last_frames.len() == resend_same_pixel_max {
                    send = false;
                    for older_frame in &last_frames {
                        if color != color_at(older_frame, x, y) {
                            send = true;
                            break;
                        }
                    }
                }

                if send {
                    let dest_addr = to_addr(Pos::new(args.offset_x + x, args.offset_y + y), color);
                    let data = make_icmpv6_packet(ethernet_info, src_ip, dest_addr);
                    data_array.push(data);
                    packet_counter += 1;
                }

                x += 1;
                if x as u16 >= width {
                    x = 0;
                    y += 1;
                }
            }

            if resend_same_pixel_max > 0 {
                while last_frames.len() >= resend_same_pixel_max {
                    last_frames.pop_front();
                }
                last_frames.push_back(buffer);
            }

            if args.noisy {
                info!("RX: Shuffling packets...");
                data_array.shuffle(&mut rng);
            }

            info!("RX: Sending frame as {} pings...", data_array.len());
            for data in data_array {
                if let Some(ref packets_per_sec) = args.packets_per_sec {
                    loop {
                        let expected_packetcount = (*packets_per_sec as f64
                            * (started_at.elapsed().as_millis() as f64 / 1000f64))
                            as u64;
                        if packet_counter > expected_packetcount {
                            std::thread::yield_now();
                        } else {
                            break;
                        }
                    }
                }
                iface.send(&data).unwrap();
                packet_counter += 1;
            }
            //iface.send(&all_data).unwrap();
            iface.flush();
            info!("RX: Sent frame as pings!");
            let elapsed_ms = last_sec.elapsed().as_millis();
            if elapsed_ms >= 1000 {
                info!(
                    "RX: Estimated effective speed {:.2} fps",
                    ((counter as f64 - last_sec_counter as f64) / (elapsed_ms as f64)) * 1000f64
                );
                last_sec = Instant::now();
                last_sec_counter = counter;
            }
            counter += 1;
        }

        info!("RX: Received: {} frames", counter);
    });

    let mut buffer = vec![0; bytes_per_frame];
    let mut succeeded = 0;
    let mut dropped = 0;

    while stdin().read_exact(&mut buffer).is_ok() {
        match tx.try_send(buffer) {
            Ok(_) => {
                info!("TX: Passed a frame!");
                succeeded += 1
            }
            Err(_) => {
                info!("TX: Dropped a frame!");
                dropped += 1
            }
        }
        buffer = vec![0; bytes_per_frame];
    }
    info!(
        "TX: Succeeded: {} frames ; Dropped: {} frames",
        succeeded, dropped
    );
    drop(tx); // Basically end of scope for sender (all senders out of scope = closed)

    thread_handle.join().unwrap();

    info!("Done!");
    Ok(())
}

fn run_image(
    args: Args,
    path: PathBuf,
    alpha_treshold: Option<u8>,
    continous: bool,
) -> Result<()> {
    let iface_name = &args.iface_name;
    let lib = rawsock::open_best_library().unwrap();
    let iface = lib.open_interface(iface_name).unwrap();
    iface.break_loop();

    let src_mac = mac_address::mac_address_by_name(iface_name)
        .unwrap()
        .ok_or(eyre!("No mac :("))
        .unwrap();
    let dest_mac = args.dest_mac;
    info!("Src Mac: {}", src_mac);
    let ethernet_info = match iface.data_link() {
        rawsock::DataLink::Ethernet => Some(EthernetInfo::new(src_mac, dest_mac)),
        _ => None,
    };

    let mut rng: rand::rngs::ThreadRng = rand::thread_rng();

    let img = if path == PathBuf::from("-") {
        let mut stdin_buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut stdin_buf)
            .context("Reading image from stdin")?;
        image::load_from_memory(&stdin_buf).context("Opening image read from stdin")?
    } else {
        image::open(path).context("Opening image")?
    };

    info!("Processing image...");
    let (mut worst_x_clip, mut worst_y_clip) = (0, 0);
    let mut data_array = Vec::<Vec<u8>>::with_capacity(img.width() as usize * img.height() as usize);
    for (x, y, pixel) in img.to_rgba8().enumerate_pixels() {
        if let Some(alpha_treshold) = alpha_treshold {
            if pixel.0[3] < alpha_treshold {
                continue;
            }
        }
        if ((x as u16) < args.min_x) || ((x as u16) > args.max_x) || ((y as u16) < args.min_y) || ((y as u16) > args.max_y) {
            continue;
        }

        let x_adj = args.offset_x + x as u16;
        let y_adj = args.offset_y + y as u16;

        let x_clip: i32 = x_adj as i32 - 1919;
        let y_clip: i32 = y_adj as i32 - 1079;
        worst_x_clip = worst_x_clip.max(x_clip);
        worst_y_clip = worst_y_clip.max(y_clip);

        if x_clip > 0 || y_clip > 0{
            continue; // Outside area. Skip
        }

        let dest_ip = to_addr(
            Pos::new(x_adj, y_adj),
            Color::new_alpha(pixel.0[0], pixel.0[1], pixel.0[2], pixel.0[3]),
        );
        data_array.push(make_icmpv6_packet(ethernet_info, args.src_ip, dest_ip));
    }

    if worst_x_clip > 0 || worst_y_clip > 0 {
        warn!("Some pixels were outside of the canvas (x clip = {worst_x_clip} and y clip = {worst_y_clip})! Automatically removed.");
    }

    let mut counter = 0;
    let mut last_sec = Instant::now();
    let mut last_sec_counter = 0;

    loop {
        if args.noisy {
            info!("Shuffling packets...");
            data_array.shuffle(&mut rng);
        }

        info!("Sending image as {} pings...", data_array.len());
        let mut packet_counter = 0;
        let started_at = Instant::now();
        for data in data_array.iter() {
            if let Some(ref packets_per_sec) = args.packets_per_sec {
                loop {
                    let expected_packetcount = (*packets_per_sec as f64
                        * (started_at.elapsed().as_millis() as f64 / 1000f64))
                        as u64;
                    if packet_counter > expected_packetcount {
                        std::thread::yield_now();
                    } else {
                        break;
                    }
                }
            }
            iface.send(data).unwrap();
            packet_counter += 1;
        }
        iface.flush();
        info!("Sent image as pings!");
        if !continous {
            break;
        } else {
            let elapsed_ms = last_sec.elapsed().as_millis();
            if elapsed_ms >= 1000 {
                info!(
                    "Estimated effective speed {:.2} fps",
                    ((counter as f64 - last_sec_counter as f64) / (elapsed_ms as f64)) * 1000f64
                );
                last_sec = Instant::now();
                last_sec_counter = counter;
            }
        }
        counter += 1;
    }
    Ok(())
}
