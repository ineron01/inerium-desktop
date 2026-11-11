use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::time::Duration;

use kanagawa_tools::sys::posix::posix_getuid;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: kanagawa-popup <popup_name> [args...]");
        std::process::exit(1);
    }

    let uid = posix_getuid();
    let sock_path = format!("/run/user/{}/kanagawa-popups.sock", uid);

    let mut connected = false;
    if let Ok(mut stream) = UnixStream::connect(&sock_path) {
        let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));
        let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));

        let msg = format!("{}\n", args.join(" "));
        if stream.write_all(msg.as_bytes()).is_ok() {
            let mut buf = [0u8; 16];
            let _ = stream.read(&mut buf);
            connected = true;
        }
    }

    if !connected {
        // Fallback: start daemon via systemd and retry connection
        let _ = Command::new("systemctl")
            .args(["--user", "start", "kanagawa-popups.service"])
            .status();

        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(mut stream) = UnixStream::connect(&sock_path) {
                let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));
                let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                let msg = format!("{}\n", args.join(" "));
                if stream.write_all(msg.as_bytes()).is_ok() {
                    let mut buf = [0u8; 16];
                    let _ = stream.read(&mut buf);
                    break;
                }
            }
        }
    }
}
