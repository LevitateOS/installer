use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

const SERVER_PORT: u16 = 8765;

// =============================================================================
// Installer-specific configuration
// =============================================================================

const SYSTEM_PROMPT: &str = r#"You are the LevitateOS installation assistant. Help users install their operating system.

CRITICAL RULES:
1. When user wants to DO something (list, format, partition, mount, create, set, install), ALWAYS call run_shell_command
2. When user CONFIRMS an action (yes, ok, proceed, continue, do it), EXECUTE the pending command via run_shell_command
3. When user asks a QUESTION (what is, how do, should I, explain), respond with text

COMMAND REFERENCE:
- List disks: lsblk
- Partition disk: sgdisk -Z /dev/X && sgdisk -n 1:0:+512M -t 1:ef00 -n 2:0:0 /dev/X
- Format EFI: mkfs.fat -F32 /dev/X1
- Format root: mkfs.ext4 /dev/X2
- Mount root: mount /dev/X2 /mnt
- Mount EFI: mkdir -p /mnt/boot/efi && mount /dev/X1 /mnt/boot/efi
- Set hostname: hostnamectl set-hostname NAME
- Set timezone: timedatectl set-timezone ZONE
- Create user: useradd -m -G wheel NAME
- Install GRUB: grub-install --target=x86_64-efi --efi-directory=/boot/efi

Only reference disks that exist in the system state above. Never hallucinate disk names."#;

const SHELL_TOOL: &str = r#"{"type":"function","function":{"name":"run_shell_command","description":"Execute a shell command for system installation tasks.","parameters":{"type":"object","properties":{"command":{"type":"string","description":"The shell command to execute"}},"required":["command"]}}}"#;

// =============================================================================
// Response types
// =============================================================================

#[derive(Deserialize)]
pub struct LlmResponse {
    pub success: bool,
    #[serde(rename = "type")]
    pub response_type: Option<String>,
    pub response: Option<String>,
    pub command: Option<String>,
    pub tool_name: Option<String>,
    pub arguments: Option<serde_json::Value>,
    pub error: Option<String>,
    pub thinking: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct LlmRequest {
    messages: Vec<ChatMessage>,
    system_prompt: String,
    system_context: String,
    tools: Vec<serde_json::Value>,
}

// =============================================================================
// System Facts Gathering
// =============================================================================

#[derive(Default)]
struct SystemFacts {
    boot_mode: String,
    network: bool,
    hostname: String,
    timezone: String,
    disks: Vec<DiskInfo>,
    users: Vec<String>,
    mounts: Vec<(String, String)>,
}

struct DiskInfo {
    device: String,
    size: String,
    model: String,
    partitions: Vec<PartitionInfo>,
}

struct PartitionInfo {
    name: String,
    size: String,
    fstype: Option<String>,
    mountpoint: Option<String>,
}

fn gather_system_facts() -> SystemFacts {
    let mut facts = SystemFacts::default();

    // Boot mode
    facts.boot_mode = if std::path::Path::new("/sys/firmware/efi/efivars").exists() {
        "UEFI".to_string()
    } else {
        "Legacy BIOS".to_string()
    };

    // Disks via lsblk
    if let Ok(output) = Command::new("lsblk")
        .args(["-J", "-o", "NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,MODEL"])
        .output()
    {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(devices) = json.get("blockdevices").and_then(|v| v.as_array()) {
                    for dev in devices {
                        if dev.get("type").and_then(|t| t.as_str()) == Some("disk") {
                            let device = format!(
                                "/dev/{}",
                                dev.get("name").and_then(|n| n.as_str()).unwrap_or("")
                            );
                            let size = dev
                                .get("size")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let model = dev
                                .get("model")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown")
                                .trim()
                                .to_string();

                            let mut partitions = Vec::new();
                            if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
                                for part in children {
                                    let name = part
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let psize = part
                                        .get("size")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let fstype = part
                                        .get("fstype")
                                        .and_then(|f| f.as_str())
                                        .map(|s| s.to_string());
                                    let mountpoint = part
                                        .get("mountpoint")
                                        .and_then(|m| m.as_str())
                                        .map(|s| s.to_string());

                                    partitions.push(PartitionInfo {
                                        name,
                                        size: psize,
                                        fstype,
                                        mountpoint,
                                    });
                                }
                            }

                            facts.disks.push(DiskInfo {
                                device,
                                size,
                                model,
                                partitions,
                            });
                        }
                    }
                }
            }
        }
    }

    // Network check
    facts.network = Command::new("ping")
        .args(["-c", "1", "-W", "2", "archlinux.org"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    // Hostname
    facts.hostname = Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Timezone
    facts.timezone = fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .map(|s| s.replace("/usr/share/zoneinfo/", ""))
        .unwrap_or_else(|| "not set".to_string());

    // Users (non-system)
    if let Ok(content) = fs::read_to_string("/etc/passwd") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 7 {
                if let Ok(uid) = parts[2].parse::<u32>() {
                    if (1000..60000).contains(&uid) {
                        facts.users.push(parts[0].to_string());
                    }
                }
            }
        }
    }

    // Collect mount info from partitions
    for disk in &facts.disks {
        for part in &disk.partitions {
            if let Some(mp) = &part.mountpoint {
                facts.mounts.push((format!("/dev/{}", part.name), mp.clone()));
            }
        }
    }

    facts
}

fn format_system_context(facts: &SystemFacts) -> String {
    let mut lines = vec!["## Current System State\n".to_string()];

    lines.push(format!("- Boot mode: {}", facts.boot_mode));
    lines.push(format!(
        "- Network: {}",
        if facts.network {
            "Connected"
        } else {
            "Not connected"
        }
    ));
    lines.push(format!("- Hostname: {}", facts.hostname));
    lines.push(format!("- Timezone: {}", facts.timezone));

    if !facts.disks.is_empty() {
        lines.push("\n## Available Disks\n".to_string());
        for disk in &facts.disks {
            let model = if disk.model.is_empty() {
                "Unknown"
            } else {
                &disk.model
            };
            lines.push(format!("- {}: {} ({})", disk.device, disk.size, model));
            for part in &disk.partitions {
                let mut info = format!("  - /dev/{}: {}", part.name, part.size);
                if let Some(fs) = &part.fstype {
                    info.push_str(&format!(" [{}]", fs));
                }
                if let Some(mp) = &part.mountpoint {
                    info.push_str(&format!(" mounted at {}", mp));
                }
                lines.push(info);
            }
        }
    }

    if !facts.mounts.is_empty() {
        lines.push("\n## Current Mounts".to_string());
        lines.push("Target partitions are mounted under /mnt".to_string());
    }

    if !facts.users.is_empty() {
        lines.push(format!("\n## Existing Users: {}", facts.users.join(", ")));
    }

    lines.join("\n")
}

fn get_valid_disks(facts: &SystemFacts) -> HashSet<String> {
    let mut valid = HashSet::new();
    for disk in &facts.disks {
        valid.insert(disk.device.clone());
        for part in &disk.partitions {
            valid.insert(format!("/dev/{}", part.name));
        }
    }
    valid
}

// =============================================================================
// Hallucination Detection
// =============================================================================

fn verify_response(response: LlmResponse, valid_disks: &HashSet<String>) -> LlmResponse {
    // Handle tool_call responses (convert to command format and verify)
    if response.response_type.as_deref() == Some("tool_call") {
        if response.tool_name.as_deref() == Some("run_shell_command") {
            if let Some(args) = &response.arguments {
                if let Some(command) = args.get("command").and_then(|c| c.as_str()) {
                    // Check for hallucinated disks
                    let re = Regex::new(r"/dev/\w+").unwrap();
                    for cap in re.captures_iter(command) {
                        let path = cap.get(0).unwrap().as_str();
                        // Allow common pseudo-devices
                        if matches!(
                            path,
                            "/dev/null" | "/dev/zero" | "/dev/urandom" | "/dev/random"
                        ) {
                            continue;
                        }
                        if !valid_disks.contains(path) {
                            // Hallucinated disk detected!
                            return LlmResponse {
                                success: true,
                                response_type: Some("text".to_string()),
                                response: Some(format!(
                                    "I couldn't find {} on this system. Let me check what disks are available.",
                                    path
                                )),
                                command: None,
                                tool_name: None,
                                arguments: None,
                                error: None,
                                thinking: response.thinking,
                            };
                        }
                    }

                    // Valid command - convert to command format
                    return LlmResponse {
                        success: true,
                        response_type: Some("command".to_string()),
                        response: None,
                        command: Some(command.to_string()),
                        tool_name: None,
                        arguments: None,
                        error: None,
                        thinking: response.thinking,
                    };
                }
            }
        }
    }

    response
}

// =============================================================================
// Toolkit Discovery
// =============================================================================

fn find_llm_toolkit() -> Result<PathBuf, String> {
    // 1. Explicit env var
    if let Ok(path) = std::env::var("LLM_TOOLKIT_PATH") {
        let p = PathBuf::from(&path);
        if p.join("llm_server.py").exists() {
            return Ok(p);
        }
        return Err(format!(
            "LLM_TOOLKIT_PATH={} does not contain llm_server.py",
            path
        ));
    }

    // 2. Relative to executable (dev environment)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for relative in &["../llm-toolkit", "../../llm-toolkit", "../../../llm-toolkit"] {
                let path = exe_dir.join(relative);
                if path.join("llm_server.py").exists() {
                    return Ok(path.canonicalize().unwrap_or(path));
                }
            }
        }
    }

    // 3. Relative to cwd (common in dev)
    let cwd_relative = PathBuf::from("llm-toolkit");
    if cwd_relative.join("llm_server.py").exists() {
        return Ok(cwd_relative.canonicalize().unwrap_or(cwd_relative));
    }

    // 4. System paths
    for sys_path in &["/usr/share/llm-toolkit", "/usr/local/share/llm-toolkit"] {
        let path = PathBuf::from(sys_path);
        if path.join("llm_server.py").exists() {
            return Ok(path);
        }
    }

    Err("llm-toolkit not found. Set LLM_TOOLKIT_PATH or install to /usr/share/llm-toolkit".into())
}

// =============================================================================
// LLM Server
// =============================================================================

pub struct LlmServer {
    process: Child,
    valid_disks: HashSet<String>,
    system_context: String,
}

impl LlmServer {
    pub fn start(model_path: &str) -> Result<Self, String> {
        // Find the toolkit
        let toolkit_path = find_llm_toolkit()?;
        let server_script = toolkit_path.join("llm_server.py");

        eprintln!("Using llm-toolkit from: {}", toolkit_path.display());

        // Gather system facts for context
        let facts = gather_system_facts();
        let system_context = format_system_context(&facts);
        let valid_disks = get_valid_disks(&facts);

        // Start the server
        let process = Command::new("python3")
            .arg(&server_script)
            .arg("--model")
            .arg(model_path)
            .arg("--port")
            .arg(SERVER_PORT.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start LLM server: {}", e))?;

        let server = LlmServer {
            process,
            valid_disks,
            system_context,
        };

        // Wait for server to be ready
        server.wait_for_ready()?;

        Ok(server)
    }

    fn wait_for_ready(&self) -> Result<(), String> {
        for _ in 0..60 {
            if TcpStream::connect(format!("127.0.0.1:{}", SERVER_PORT)).is_ok() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err("LLM server failed to start within 30 seconds".to_string())
    }

    pub fn query(&self, messages: &[ChatMessage]) -> Result<LlmResponse, String> {
        let tools: Vec<serde_json::Value> = vec![serde_json::from_str(SHELL_TOOL).unwrap()];

        let request = LlmRequest {
            messages: messages.to_vec(),
            system_prompt: SYSTEM_PROMPT.to_string(),
            system_context: self.system_context.clone(),
            tools,
        };
        let body = serde_json::to_string(&request).map_err(|e| e.to_string())?;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", SERVER_PORT))
            .map_err(|e| format!("Failed to connect to LLM server: {}", e))?;

        let http_request = format!(
            "POST /query HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );

        stream
            .write_all(http_request.as_bytes())
            .map_err(|e| format!("Failed to send request: {}", e))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Parse HTTP response
        let body_start = response.find("\r\n\r\n").ok_or("Invalid HTTP response")?;
        let json_body = &response[body_start + 4..];

        let raw_response: LlmResponse = serde_json::from_str(json_body)
            .map_err(|e| format!("Failed to parse response: {}\nBody: {}", e, json_body))?;

        // Verify response for hallucinations
        Ok(verify_response(raw_response, &self.valid_disks))
    }
}

impl Drop for LlmServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}
