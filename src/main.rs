mod server;
mod service;
mod logger;
mod proxy;

use server::Config;
use std::io::{self, Write};

const PID_FILE: &str = "/tmp/nimbus.pid";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd    = args.get(1).map(|s| s.as_str());
    let config = Config {
        root:       flag(&args, "--root").unwrap_or_else(|| "/var/www".to_string()),
        production: args.windows(2).any(|w| w[0] == "--env" && w[1] == "production"),
        php:        args.iter().any(|a| a == "--php"),
    };

    match cmd {
        Some("stop")      => service::stop(PID_FILE),
        Some("status")    => service::status(PID_FILE),
        Some("uninstall") => service::uninstall(),
        Some("help")      => print_help(),
        Some("proxy") => {
            let ip     = flag(&args, "-ip").unwrap_or_else(|| prompt("IP"));
            let port   = flag(&args, "-port").unwrap_or_else(|| prompt("Port"));
            let config = flag(&args, "-config").unwrap_or_else(|| prompt("Routes config file"));
            let routes = proxy::load_routes(&config);
            proxy::run(&format!("{ip}:{port}"), routes);
        }
        Some("show-ip")   => show_ip(),
        Some("logs")      => show_logs(),
        Some("install") => {
            let ip   = flag(&args, "-ip").unwrap_or_else(|| prompt("IP"));
            let port = flag(&args, "-port").unwrap_or_else(|| prompt("Port"));
            service::install(&ip, &port, config);
        }
        Some("--server") => {
            let ip   = flag(&args, "-ip").unwrap();
            let port = flag(&args, "-port").unwrap();
            server::serve(&format!("{ip}:{port}"), config);
        }
        _ => {
            let ip   = flag(&args, "-ip").unwrap_or_else(|| prompt("IP"));
            let port = flag(&args, "-port").unwrap_or_else(|| prompt("Port"));
            let addr = format!("{ip}:{port}");
            let exe  = std::env::current_exe().unwrap();
            let mut spawn_args = vec!["--server", "-ip", &ip, "-port", &port,
                                       "--root", &config.root];
            if config.production { spawn_args.extend_from_slice(&["--env", "production"]); }
            if config.php        { spawn_args.push("--php"); }
            let child = std::process::Command::new(exe)
                .args(&spawn_args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("failed to start server process");
            std::fs::write(PID_FILE, format!("{}\n{addr}", child.id()))
                .expect("failed to write PID file");
            print_banner();
            println!("Starting Nimbus on http://{addr}");
            if config.php { println!("PHP support enabled"); }
        }
    }
}

fn print_banner() {
    println!(r#"
 _   _ _           _
| \ | (_)_ __ ___ | |__  _   _ ___
|  \| | | '_ ` _ \| '_ \| | | / __|
| |\  | | | | | | | |_) | |_| \__ \
|_| \_|_|_| |_| |_|_.__/ \__,_|___/
                   a simple web server
"#);
}

fn show_logs() {
    std::process::Command::new("tail")
        .args(["-f", logger::LOG_FILE])
        .status()
        .expect("failed to run tail");
}

fn show_ip() {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").expect("failed to bind socket");
    socket.connect("8.8.8.8:80").expect("failed to connect");
    match socket.local_addr() {
        Ok(addr) => println!("{}", addr.ip()),
        Err(_)   => eprintln!("Could not determine IP address"),
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn prompt(label: &str) -> String {
    print!("{label}: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn print_help() {
    print!(concat!(
        "Nimbus - a simple web server\n",
        "\n",
        "USAGE:\n",
        "  nimbus [-ip <address>] [-port <number>] [--php] [--env production]\n",
        "\n",
        "OPTIONS:\n",
        "  -ip <address>      IP to listen on  (e.g. 0.0.0.0 for all, 127.0.0.1 for local)\n",
        "  -port <number>     Port to listen on (e.g. 8080)\n",
        "  --root <path>      Directory to serve files from (default: /var/www)\n",
        "  --php              Enable PHP support\n",
        "  --env production   Hide error details from visitors\n",
        "\n",
        "COMMANDS:\n",
        "  stop                        Stop the running server\n",
        "  proxy -ip X -port X -config <file>   Start the reverse proxy\n",
        "  logs                        Follow the request log live\n",
        "  show-ip                     Show the IP address of this machine\n",
        "  status                      Show whether Nimbus is running\n",
        "  install [-ip X] [-port X] [--php] [--env production]\n",
        "                              Install as a service that starts on boot\n",
        "  uninstall                   Remove from startup\n",
        "  help                        Show this message\n",
        "\n",
        "EXAMPLES:\n",
        "  nimbus -ip 0.0.0.0 -port 8080\n",
        "  nimbus -ip 0.0.0.0 -port 8080 --php\n",
        "  nimbus -ip 0.0.0.0 -port 8080 --php --env production\n",
        "  nimbus stop\n",
        "  sudo nimbus install -ip 0.0.0.0 -port 8080 --php --env production\n",
        "  sudo nimbus uninstall\n",
    ));
}
