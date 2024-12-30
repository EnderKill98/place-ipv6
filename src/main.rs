//! https://v6staging.sys42.net/
//! https://github.com/Xevion/v6-place

use std::{
    collections::VecDeque,
    io::{stdin, Read},
    path::PathBuf,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread,
    time::Instant,
};
use std::io::{BufWriter, Write};
use std::net::TcpStream;
use clap::{Parser, Subcommand};
use color_eyre::eyre::{bail, Context};
use color_eyre::Result;
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

    /// Destination address and port
    #[arg(short = 'd', long, default_value = "table.apokalypse.email:1337")]
    destination_addr: String,
    /// If set, limit transmission speed to the given pixels/sec.
    #[arg(short = 'r', long)]
    pixels_per_sec: Option<u32>,
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

fn run_rawpipe_stdin(mut args: Args, resend_same_pixel_max: usize, width: u16, height: u16, has_alpha: bool) -> Result<()> {
    let bytes_per_pixel = if has_alpha { 4 } else { 3 };
    let bytes_per_frame: usize = ((width as u32) * (height as u32) * bytes_per_pixel) as usize;

    if args.offset_x + width > 3840 {
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
        let mut conn = BufWriter::with_capacity(10000000, TcpStream::connect(args.destination_addr).unwrap());

        // Apply offset and remove it for later stuff
        conn.write_all(format!("OFFSET {} {}", args.offset_x, args.offset_y).as_bytes()).unwrap();
        conn.flush().unwrap();

        args.offset_x = 0;
        args.offset_y = 0;

        let mut packet_counter;
        info!("RX: Ready...");
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
                    let line = format!("PX {} {} {:02x}{:02x}{:02x}\n", args.offset_x + x, args.offset_y + y, color.red, color.green, color.blue);
                    let data = Vec::from(line.as_bytes());
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
                if let Some(ref packets_per_sec) = args.pixels_per_sec {
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
                conn.write_all(&data).unwrap();
                packet_counter += 1;
            }
            conn.flush().unwrap();

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
    mut args: Args,
    path: PathBuf,
    alpha_treshold: Option<u8>,
    continous: bool,
) -> Result<()> {
    let mut rng: rand::rngs::ThreadRng = rand::thread_rng();
    let mut conn = BufWriter::with_capacity(10000000, TcpStream::connect(args.destination_addr)?);

    // Apply offset and remove it for later stuff
    conn.write_all(format!("OFFSET {} {}", args.offset_x, args.offset_y).as_bytes())?;
    conn.flush()?;
    args.offset_x = 0;
    args.offset_y = 0;

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

        let line = format!("PX {} {} {:02x}{:02x}{:02x}\n", x_adj, y_adj, pixel.0[0], pixel.0[1], pixel.0[2]);
        //println!("{line}");
        let data = Vec::from(line.as_bytes());
        data_array.push(data);
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
            if let Some(ref packets_per_sec) = args.pixels_per_sec {
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
            conn.write_all(data)?;
            packet_counter += 1;
        }
        conn.flush()?;
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
