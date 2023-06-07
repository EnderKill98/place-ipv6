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
use color_eyre::eyre::{eyre, Context};
use color_eyre::Result;
use fxhash::FxHashSet;
use image::GenericImageView;
use mac_address::MacAddress;
use place_ipv6::*;
use rand::seq::SliceRandom;

#[macro_use]
extern crate log;

#[derive(Subcommand, Clone)]
enum Commands {
    /// Read a stream of rgb24 raw frames in 256x256 size (usually from ffmpeg with the flags `-pix_fmt rgb24 -f rawvideo pipe:1`)
    Rgb24Stdin {
        /// Stop sending a pixel if it was the same for given amound of processed/sent frames (0 = send always regardless). HIGH NUMBERS CAN FILL YOUR RAM OVER TIME!
        #[arg(short = 'r', long, default_value = "0")]
        resend_same_pixel_max: usize,
    },
    Image {
        /// Path to image that should be displayed
        path: PathBuf,

        /// At what alpha value a pixel should be sent (if image has an alpha channel). If missing, alpha will be ignored.
        #[arg(short = 'a', long)]
        alpha_threshold: Option<u8>,

        /// If set, image will be sent in 512x512 resolution instead of 256x256
        #[arg(short = 'f', long, action)]
        full_resolution: bool,

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

    /// Skip all pixels smaller than given value (at 512x512 resolution)
    #[arg(short = 'x', long, default_value = "0")]
    min_x: u16,
    /// Skip all pixels bigger than given value (at 512x512 resolution)
    #[arg(short = 'X', long, default_value = "512")]
    max_x: u16,

    /// Skip all pixels smaller than given value (at 512x512 resolution)
    #[arg(short = 'y', long, default_value = "0")]
    min_y: u16,
    /// Skip all pixels bigger than given value (at 512x512 resolution)
    #[arg(short = 'Y', long, default_value = "512")]
    max_y: u16,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }

    env_logger::builder().format_timestamp_millis().init();

    match args.command {
        Commands::Rgb24Stdin {
            resend_same_pixel_max,
        } => run_rgb24_stdin(args.clone(), resend_same_pixel_max),
        Commands::Image {
            ref path,
            alpha_threshold,
            full_resolution,
            continuous,
        } => run_image(
            args.clone(),
            path.clone(),
            alpha_threshold,
            full_resolution,
            continuous,
        ),
    }
}

fn run_rgb24_stdin(args: Args, resend_same_pixel_max: usize) -> Result<()> {
    const WIDTH: u32 = 256;
    const HEIGHT: u32 = 256;

    const BYTES_PER_FRAME: usize = (WIDTH * HEIGHT * 3) as usize;

    let (tx, rx): (
        SyncSender<[u8; BYTES_PER_FRAME]>,
        Receiver<[u8; BYTES_PER_FRAME]>,
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

        let mut last_frames: VecDeque<[u8; HEIGHT as usize * WIDTH as usize * 3]> =
            VecDeque::with_capacity(resend_same_pixel_max);
        let color_at = |buf: &[u8], x: u16, y: u16| {
            let index = 3 * WIDTH as usize * y as usize + 3 * x as usize;
            Color::new(buf[index], buf[index + 1], buf[index + 2])
        };

        for buffer in rx {
            let mut x = 0;
            let mut y = 0;
            let started_at = Instant::now();
            packet_counter = 0;

            let mut data_array = Vec::new();

            info!("RX: Processing frame...");
            for buffer_index in (0..buffer.len()).step_by(3) {
                let color = Color::new(
                    buffer[buffer_index],
                    buffer[buffer_index + 1],
                    buffer[buffer_index + 2],
                );

                let mut send = true;

                if x * 2 < args.min_x
                    || x * 2 > args.max_x
                    || y * 2 < args.min_y
                    || y * 2 > args.max_y
                {
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
                    let dest_addr = to_addr(Pos::new(x * 2, y * 2), color, Size::Area2x2);
                    let data = make_icmpv6_packet(ethernet_info, src_ip, dest_addr);
                    data_array.push(data);
                    packet_counter += 1;
                }

                x += 1;
                if x as u32 >= WIDTH {
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

    let mut buffer = [0u8; BYTES_PER_FRAME];
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
    full_resolution: bool,
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

    let mut img = image::open(path).context("Opening image")?;

    if !full_resolution && (img.width() != 256 || img.height() != 256) {
        info!("Resizing image to 256x256...");
        img = img.resize_to_fill(256, 256, image::imageops::FilterType::Lanczos3);
    }
    if full_resolution && (img.width() != 512 || img.height() != 512) {
        info!("Resizing image to 256x256...");
        img = img.resize_to_fill(512, 512, image::imageops::FilterType::Lanczos3);
    }

    info!("Processing image...");
    let pixel_size = if full_resolution {
        Size::SinglePixel
    } else {
        Size::Area2x2
    };
    let pos_multiplier = if full_resolution { 1 } else { 2 };
    let mut data_array = Vec::with_capacity(img.width() as usize * img.height() as usize);
    let mut skip_pixels = FxHashSet::default();
    let mut optimized_pixels: usize = 0;
    for (x, y, pixel) in img.to_rgba8().enumerate_pixels() {
        if let Some(alpha_treshold) = alpha_treshold {
            if pixel.0[3] < alpha_treshold {
                continue;
            }
        }
        if skip_pixels.contains(&(x, y)) {
            continue;
        }
        let x_adj = x as u16 * pos_multiplier;
        let y_adj = y as u16 * pos_multiplier;

        let mut pixel_size = pixel_size;
        if full_resolution
            && x < 512 - 1
            && y < 512 - 1
            && &img.get_pixel(x + 1, y) == pixel
            && &img.get_pixel(x, y + 1) == pixel
            && &img.get_pixel(x + 1, y + 1) == pixel
            && !(x_adj < args.min_x
                || x_adj > args.max_x
                || y_adj < args.min_y
                || y_adj > args.max_y)
            && !(x_adj + 1 < args.min_x
                || x_adj + 1 > args.max_x
                || y_adj + 1 < args.min_y
                || y_adj + 1 > args.max_y)
        {
            pixel_size = Size::Area2x2;
            skip_pixels.insert((x + 1, y));
            skip_pixels.insert((x, y + 1));
            skip_pixels.insert((x + 1, y + 1));
            optimized_pixels += 1;
        }
        if x_adj < args.min_x || x_adj > args.max_x || y_adj < args.min_y || y_adj > args.max_y {
            continue;
        }
        let dest_ip = to_addr(
            Pos::new(x_adj, y_adj),
            Color::new(pixel.0[0], pixel.0[1], pixel.0[2]),
            pixel_size,
        );
        data_array.push(make_icmpv6_packet(ethernet_info, args.src_ip, dest_ip));
    }
    if optimized_pixels > 0 {
        info!("Optimized {optimized_pixels} pixels into 2x2 areas!");
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
        for data in &data_array {
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
