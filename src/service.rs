use std::fs;
use std::path::Path;
use crate::server::Config;

const SERVICE_FILE: &str = "/etc/systemd/system/nimbus.service";

pub fn install(ip: &str, port: &str, config: Config) {
    let exe = std::env::current_exe().unwrap();
    let mut flags = format!(" --root {}", config.root);
    if config.php        { flags.push_str(" --php"); }
    if config.production { flags.push_str(" --env production"); }
    let content = format!(
        "[Unit]\n\
         Description=Nimbus Web Server\n\
         After=network.target\n\
         \n\
         [Service]\n\
         ExecStart={} --server -ip {} -port {}{}\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        exe.display(), ip, port, flags
    );
    fs::write(SERVICE_FILE, content)
        .expect("failed to write service file — try running with sudo");
    systemctl(&["daemon-reload"]);
    systemctl(&["enable", "nimbus"]);
    systemctl(&["start", "nimbus"]);
    println!("Nimbus installed and started. It will now start automatically on boot.");
}

pub fn uninstall() {
    systemctl(&["stop", "nimbus"]);
    systemctl(&["disable", "nimbus"]);
    let _ = fs::remove_file(SERVICE_FILE);
    systemctl(&["daemon-reload"]);
    println!("Nimbus uninstalled and removed from startup.");
}

pub fn stop(pid_file: &str) {
    if Path::new(SERVICE_FILE).exists() {
        systemctl(&["stop", "nimbus"]);
        println!("Nimbus stopped");
        return;
    }
    match fs::read_to_string(pid_file) {
        Ok(content) => {
            let pid = content.lines().next().unwrap_or("").trim().to_string();
            std::process::Command::new("kill").arg(&pid).status().unwrap();
            let _ = fs::remove_file(pid_file);
            println!("Nimbus stopped");
        }
        Err(_) => eprintln!("Nimbus is not running"),
    }
}

pub fn status(pid_file: &str) {
    if Path::new(SERVICE_FILE).exists() {
        let addr = service_addr();
        println!("Nimbus is installed as a service — listening on http://{addr}");
        systemctl(&["status", "nimbus"]);
        return;
    }
    match fs::read_to_string(pid_file) {
        Ok(content) => {
            let mut lines = content.lines();
            let pid  = lines.next().unwrap_or("?");
            let addr = lines.next().unwrap_or("unknown");
            println!("Nimbus is running on http://{addr} (PID {pid})");
        }
        Err(_) => println!("Nimbus is not running"),
    }
}

fn service_addr() -> String {
    let Ok(content) = fs::read_to_string(SERVICE_FILE) else {
        return "unknown".to_string();
    };
    let line = content.lines().find(|l| l.contains("ExecStart")).unwrap_or("");
    let parts: Vec<&str> = line.split_whitespace().collect();
    let get = |flag: &str| parts.windows(2).find(|w| w[0] == flag).map(|w| w[1]).unwrap_or("?");
    format!("{}:{}", get("-ip"), get("-port"))
}

fn systemctl(args: &[&str]) {
    std::process::Command::new("systemctl").args(args).status()
        .unwrap_or_else(|_| panic!("failed to run systemctl {}", args[0]));
}
