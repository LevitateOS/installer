//! VM management for testing the installer.
//!
//! Uses QEMU with a Fedora cloud image and virtfs for instant file sharing.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

const SSH_PORT: u16 = 2223; // Different from recipe VM (2222)
const SSH_USER: &str = "dev";
const SSH_HOST: &str = "localhost";
const DISK_SIZE: &str = "20G";
// HARD REQUIREMENT: Must match kickstart releasever (see REQUIREMENTS.md)
const FEDORA_VERSION: &str = "43";
const FEDORA_IMAGE_URL: &str = "https://download.fedoraproject.org/pub/fedora/linux/releases/43/Cloud/x86_64/images/Fedora-Cloud-Base-Generic-43-1.6.x86_64.qcow2";

// SSH timeout constants (in seconds)
const SSH_TIMEOUT_PROBE: u16 = 2;  // Quick probe during wait_for_ssh loop
const SSH_TIMEOUT_CHECK: u16 = 3;  // Status check (try_ssh)
const SSH_TIMEOUT_RUN: u16 = 10;   // Command execution (run_ssh)

/// Builder for SSH commands - consolidates common SSH configuration
struct SshCommand {
    timeout: Option<u16>,
    batch_mode: bool,
    force_pty: bool,
    command: Option<String>,
}

impl SshCommand {
    fn new() -> Self {
        Self {
            timeout: None,
            batch_mode: false,
            force_pty: false,
            command: None,
        }
    }

    /// Set connection timeout in seconds
    fn timeout(mut self, secs: u16) -> Self {
        self.timeout = Some(secs);
        self
    }

    /// Enable batch mode (no password prompts, fail on auth issues)
    fn batch_mode(mut self) -> Self {
        self.batch_mode = true;
        self
    }

    /// Force PTY allocation (for interactive commands like tmux)
    fn force_pty(mut self) -> Self {
        self.force_pty = true;
        self
    }

    /// Set the command to run (None for interactive shell)
    fn command(mut self, cmd: &str) -> Self {
        self.command = Some(cmd.to_string());
        self
    }

    /// Build the SSH command arguments
    fn build_args(self) -> Vec<String> {
        let mut args = vec![
            "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
            "-o".to_string(), "UserKnownHostsFile=/dev/null".to_string(),
            "-o".to_string(), "LogLevel=ERROR".to_string(),
            "-o".to_string(), format!("IdentityFile={}", ssh_key_path().display()),
            "-o".to_string(), "IdentitiesOnly=yes".to_string(),
            "-p".to_string(), SSH_PORT.to_string(),
        ];

        if let Some(timeout) = self.timeout {
            args.extend(["-o".to_string(), format!("ConnectTimeout={}", timeout)]);
        }

        if self.batch_mode {
            args.extend(["-o".to_string(), "BatchMode=yes".to_string()]);
        }

        if self.force_pty {
            args.push("-t".to_string());
        }

        args.push(format!("{}@{}", SSH_USER, SSH_HOST));

        if let Some(cmd) = self.command {
            args.push(cmd);
        }

        args
    }

    /// Build and return a Command ready to execute
    fn build(self) -> Command {
        let args = self.build_args();
        let mut cmd = Command::new("ssh");
        cmd.args(&args);
        cmd
    }
}

/// Get the installer directory (parent of xtask)
fn installer_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Get the LevitateOS project root
fn project_root() -> PathBuf {
    installer_root().parent().unwrap().to_path_buf()
}

/// VM files directory
fn vm_dir() -> PathBuf {
    let dir = installer_root().join(".vm");
    fs::create_dir_all(&dir).ok();
    dir
}

fn disk_image() -> PathBuf {
    vm_dir().join("fedora-installer-dev.qcow2")
}

fn base_image() -> PathBuf {
    vm_dir().join("fedora-cloud-base.qcow2")
}

fn pid_file() -> PathBuf {
    vm_dir().join("qemu.pid")
}

fn monitor_socket() -> PathBuf {
    vm_dir().join("qemu-monitor.sock")
}

fn ssh_key_path() -> PathBuf {
    vm_dir().join("id_ed25519")
}

/// Check if QEMU is available
fn check_qemu() -> Result<()> {
    which::which("qemu-system-x86_64")
        .context("qemu-system-x86_64 not found. Install QEMU.")?;
    Ok(())
}

/// Check if KVM is available
fn check_kvm() -> Result<()> {
    let kvm = std::path::Path::new("/dev/kvm");
    if !kvm.exists() {
        bail!("/dev/kvm not found. Is KVM enabled?\n  - Check BIOS for virtualization (VT-x/AMD-V)\n  - Run: sudo modprobe kvm_intel (or kvm_amd)");
    }
    // Check if accessible
    if fs::File::open(kvm).is_err() {
        bail!("/dev/kvm not accessible. Add yourself to kvm group:\n  sudo usermod -aG kvm $USER\n  (then log out and back in)");
    }
    Ok(())
}

/// Check if a port is available
fn check_port_available(port: u16) -> Result<()> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => Ok(()),
        Err(_) => bail!(
            "Port {} is already in use.\n  Another VM running? Check: cargo xtask vm status\n  Or something else: lsof -i :{}",
            port, port
        ),
    }
}

/// Check if VM is running (and clean up stale PID file if not)
fn is_running() -> bool {
    let pf = pid_file();
    if let Ok(pid_str) = fs::read_to_string(&pf) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            if std::path::Path::new(&format!("/proc/{}", pid)).exists() {
                return true;
            }
        }
        // Stale PID file - clean it up
        let _ = fs::remove_file(&pf);
        let _ = fs::remove_file(monitor_socket());
    }
    false
}

/// Generate SSH key pair for passwordless auth
fn ensure_ssh_key() -> Result<()> {
    let key_path = ssh_key_path();
    if key_path.exists() {
        return Ok(());
    }

    println!("Generating SSH key for VM access...");
    let status = Command::new("ssh-keygen")
        .args([
            "-t", "ed25519",
            "-f", &key_path.display().to_string(),
            "-N", "", // No passphrase
            "-q",
        ])
        .status()
        .context("Failed to generate SSH key")?;

    if !status.success() {
        bail!("Failed to generate SSH key");
    }

    // Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// Get the public key content
fn get_ssh_pubkey() -> Result<String> {
    let pubkey_path = ssh_key_path().with_extension("pub");
    fs::read_to_string(&pubkey_path)
        .context("Failed to read SSH public key")
}

/// Wait for SSH to become available (with timeout)
fn wait_for_ssh(timeout_secs: u32) -> Result<()> {
    print!("Waiting for SSH");
    std::io::stdout().flush().ok();

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs as u64);

    while start.elapsed() < timeout {
        let result = SshCommand::new()
            .timeout(SSH_TIMEOUT_PROBE)
            .batch_mode()
            .command("true")
            .build()
            .output();

        if let Ok(output) = result {
            if output.status.success() {
                println!(" ready!");
                return Ok(());
            }
        }

        print!(".");
        std::io::stdout().flush().ok();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    println!();
    bail!("SSH not available after {}s. VM might still be booting.\n  Check: cargo xtask vm status\n  Logs: tail installer/.vm/serial.log", timeout_secs);
}

/// Try SSH command, return true if successful
fn try_ssh(command: &str) -> bool {
    SshCommand::new()
        .timeout(SSH_TIMEOUT_CHECK)
        .batch_mode()
        .command(command)
        .build()
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run SSH command and get output
fn run_ssh(command: &str) -> Result<String> {
    let output = SshCommand::new()
        .timeout(SSH_TIMEOUT_RUN)
        .batch_mode()
        .command(command)
        .build()
        .output()
        .context("Failed to run SSH command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("SSH command failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Setup/download the Fedora cloud image
pub fn setup(force: bool) -> Result<()> {
    check_qemu()?;
    which::which("curl").context("curl not found. Install curl.")?;
    which::which("qemu-img").context("qemu-img not found")?;

    // Don't allow setup while VM is running
    if is_running() {
        bail!("VM is running. Stop it first with 'cargo xtask vm stop'");
    }

    // Generate SSH key
    ensure_ssh_key()?;

    let disk = disk_image();
    let base = base_image();

    if disk.exists() && !force {
        println!("Disk image already exists at {:?}", disk);
        println!("Use --force to recreate, or 'cargo xtask vm reset' to reset to clean state.");
        return Ok(());
    }

    println!("=== Installer Dev VM Setup ===\n");

    // Only download if base image doesn't exist
    if !base.exists() || force {
        println!("[1/3] Downloading Fedora {} cloud image (~500MB)...", FEDORA_VERSION);
        let status = Command::new("curl")
            .args([
                "-L",
                "--progress-bar",
                "-o", &base.display().to_string(),
                FEDORA_IMAGE_URL,
            ])
            .status()
            .context("Failed to run curl")?;

        if !status.success() {
            bail!("Failed to download Fedora cloud image");
        }
    } else {
        println!("[1/3] Using cached base image...");
    }

    // Create a copy for our VM (so we can reset easily)
    println!("[2/3] Creating VM disk...");
    fs::copy(&base, &disk)?;

    println!("[3/3] Resizing disk to {}...", DISK_SIZE);
    let status = Command::new("qemu-img")
        .args(["resize", &disk.display().to_string(), DISK_SIZE])
        .status()?;

    if !status.success() {
        bail!("Failed to resize disk image");
    }

    // Always regenerate cloud-init ISO
    create_cloud_init_iso()?;

    println!("\n=== Setup Complete ===\n");
    println!("Fedora cloud image ready: {:?}", disk);
    println!();
    println!("Next: cargo xtask vm start --detach");
    println!();

    Ok(())
}

/// Reset VM disk to clean state (without re-downloading)
pub fn reset() -> Result<()> {
    if is_running() {
        bail!("VM is running. Stop it first with 'cargo xtask vm stop'");
    }

    let disk = disk_image();
    let base = base_image();

    if !base.exists() {
        bail!("Base image not found. Run 'cargo xtask vm setup' first.");
    }

    // Ensure SSH key exists
    ensure_ssh_key()?;

    println!("Resetting VM disk to clean state...");

    // Remove old disk
    if disk.exists() {
        fs::remove_file(&disk)?;
    }

    // Copy fresh from base
    fs::copy(&base, &disk)?;

    // Resize
    let status = Command::new("qemu-img")
        .args(["resize", &disk.display().to_string(), DISK_SIZE])
        .status()?;

    if !status.success() {
        bail!("Failed to resize disk image");
    }

    // Regenerate cloud-init
    create_cloud_init_iso()?;

    println!("Done! VM reset to clean state.");
    println!("Next: cargo xtask vm start --detach");

    Ok(())
}

/// Create cloud-init ISO with bun + tmux setup
fn create_cloud_init_iso() -> Result<()> {
    which::which("genisoimage")
        .or_else(|_| which::which("mkisofs"))
        .context("genisoimage or mkisofs not found. Install genisoimage package.")?;

    let pubkey = get_ssh_pubkey()?;

    let cloud_dir = vm_dir().join("cloud-init");
    fs::create_dir_all(&cloud_dir)?;

    // meta-data
    let meta_data = cloud_dir.join("meta-data");
    fs::write(&meta_data, "instance-id: installer-dev\nlocal-hostname: installer-dev\n")?;

    // user-data with SSH key, bun + tmux installation
    let user_data = cloud_dir.join("user-data");
    let user_data_content = format!(r#"#cloud-config
users:
  - name: dev
    plain_text_passwd: dev
    lock_passwd: false
    sudo: ALL=(ALL) NOPASSWD:ALL
    groups: wheel
    shell: /bin/bash
    ssh_authorized_keys:
      - {}

ssh_pwauth: true
disable_root: false

packages:
  - tmux
  - unzip

write_files:
  - path: /etc/os-release
    content: |
      NAME="LevitateOS"
      VERSION="(pre-release)"
      ID=levitate
      ID_LIKE=fedora
      PRETTY_NAME="LevitateOS (pre-release)"
      HOME_URL="https://github.com/nickvidal/LevitateOS"

  - path: /etc/issue
    content: |
      LevitateOS (pre-release)

      Run: levitate-installer

  - path: /etc/motd
    content: |

      LevitateOS (pre-release)

      Run: levitate-installer


  - path: /etc/profile.d/levitate.sh
    content: |
      export PATH="$HOME/.bun/bin:$HOME/installer/bin:$PATH"
      export TERM=xterm-256color

  - path: /etc/systemd/system/getty@tty1.service.d/autologin.conf
    owner: root:root
    permissions: '0644'
    content: |
      [Service]
      ExecStart=
      ExecStart=-/sbin/agetty -o '-p -f -- \\u' --autologin root - $TERM

runcmd:
  - systemctl enable --now sshd
  # Remove Fedora branding and SSH vsock messages
  - rm -rf /etc/issue.d /usr/lib/issue.d /run/issue.d
  - rm -f /etc/fedora-release /etc/system-release
  - echo 'LevitateOS (pre-release)' > /etc/system-release
  # Apply autologin config (getty started before write_files completed)
  - systemctl daemon-reload
  - systemctl restart getty@tty1
  # Suppress kernel messages on console
  - dmesg -n 1
  - echo 'kernel.printk = 1 1 1 1' >> /etc/sysctl.d/99-quiet-console.conf
  # Setup root environment
  - echo 'export PATH="/root/.bun/bin:$PATH"' >> /root/.bashrc
  - echo 'export TERM=xterm-256color' >> /root/.bashrc
  # Setup virtfs mount
  - mkdir -p /mnt/share
  - chown dev:dev /mnt/share
  - echo 'share /mnt/share 9p trans=virtio,nofail 0 0' >> /etc/fstab
  - mount /mnt/share || true
  # Install bun for root (installer runs as root)
  - curl -fsSL https://bun.sh/install | bash
  - ln -sf /root/.bun/bin/bun /usr/local/bin/bun
  # Also install for dev user (for SSH access)
  - su - dev -c 'curl -fsSL https://bun.sh/install | bash'
"#, pubkey.trim());

    fs::write(&user_data, user_data_content)?;

    // Generate ISO
    let iso_path = vm_dir().join("cloud-init.iso");
    let iso_tool = which::which("genisoimage")
        .unwrap_or_else(|_| which::which("mkisofs").unwrap());

    let output = Command::new(&iso_tool)
        .args([
            "-output", &iso_path.display().to_string(),
            "-volid", "cidata",
            "-joliet",
            "-rock",
            &cloud_dir.display().to_string(),
        ])
        .output()
        .context("Failed to create cloud-init ISO")?;

    if !output.status.success() {
        bail!("Failed to create cloud-init ISO: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

/// Start the VM (GUI by default)
pub fn start(gui: bool, memory: u32, cpus: u32) -> Result<()> {
    check_qemu()?;
    check_kvm()?;

    if is_running() {
        println!("VM is already running.");
        println!("  SSH: cargo xtask vm ssh");
        println!("  Stop: cargo xtask vm stop");
        return Ok(());
    }

    // Check port before trying to start
    check_port_available(SSH_PORT)?;

    let disk = disk_image();
    if !disk.exists() {
        bail!(
            "Disk image not found.\nRun 'cargo xtask vm setup' first."
        );
    }

    // Ensure we have SSH key
    ensure_ssh_key()?;

    let cloud_init_iso = vm_dir().join("cloud-init.iso");
    if !cloud_init_iso.exists() {
        create_cloud_init_iso()?;
    }

    let project = project_root();

    let mut args = vec![
        "-enable-kvm".to_string(),
        "-cpu".to_string(), "host".to_string(),
        "-m".to_string(), format!("{}M", memory),
        "-smp".to_string(), format!("{}", cpus),
        // Main disk
        "-drive".to_string(),
        format!("file={},format=qcow2,if=virtio", disk.display()),
        // Cloud-init config
        "-drive".to_string(),
        format!("file={},format=raw,if=virtio,readonly=on", cloud_init_iso.display()),
        // Network with SSH forwarding
        "-netdev".to_string(),
        format!("user,id=net0,hostfwd=tcp::{}-:22", SSH_PORT),
        "-device".to_string(), "virtio-net-pci,netdev=net0".to_string(),
        // Virtfs sharing - mount entire project
        "-virtfs".to_string(),
        format!("local,path={},mount_tag=share,security_model=none,multidevs=remap", project.display()),
        // Monitor socket for control
        "-monitor".to_string(),
        format!("unix:{},server,nowait", monitor_socket().display()),
        // PID file
        "-pidfile".to_string(), pid_file().display().to_string(),
    ];

    if gui {
        // GUI mode (default)
        args.extend([
            "-device".to_string(), "virtio-vga-gl".to_string(),
            "-display".to_string(), "gtk,gl=on".to_string(),
            "-device".to_string(), "virtio-keyboard".to_string(),
            "-device".to_string(), "virtio-mouse".to_string(),
            "-serial".to_string(), format!("file:{}", vm_dir().join("serial.log").display()),
        ]);
    } else {
        // Headless mode (background)
        args.extend([
            "-display".to_string(), "none".to_string(),
            "-serial".to_string(), format!("file:{}", vm_dir().join("serial.log").display()),
            "-daemonize".to_string(),
        ]);
    }

    println!("Starting Installer Dev VM...");
    println!("  Memory: {} MB", memory);
    println!("  CPUs: {}", cpus);
    println!("  Shared: {} -> /mnt/share", project.display());
    println!();

    if gui {
        println!("Opening QEMU window...");
        println!("  First boot: ~1 min for cloud-init");
        println!("  Auto-login: root");
        println!();
        println!("From host: cargo xtask vm sync");
        println!();
    } else {
        println!("Running in background (headless)...");
    }

    let status = Command::new("qemu-system-x86_64")
        .args(&args)
        .status()
        .context("Failed to start QEMU")?;

    if !gui {
        // Headless mode - daemonized
        std::thread::sleep(std::time::Duration::from_millis(500));
        if is_running() {
            println!("VM started in background.");
            println!();
            println!("Next: cargo xtask vm sync && cargo xtask vm ssh");
        } else {
            bail!("Failed to start VM in background");
        }
    } else if !status.success() {
        bail!("QEMU exited with error");
    }

    Ok(())
}

/// Stop the VM
pub fn stop() -> Result<()> {
    if !is_running() {
        println!("VM is not running.");
        return Ok(());
    }

    // Try graceful shutdown via monitor socket
    let monitor = monitor_socket();
    if monitor.exists() {
        if which::which("socat").is_ok() {
            println!("Sending shutdown signal...");
            let _ = Command::new("sh")
                .args(["-c", &format!("echo 'system_powerdown' | socat - UNIX-CONNECT:{}", monitor.display())])
                .status();
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    }

    // Force kill if still running
    if is_running() {
        if let Ok(pid_str) = fs::read_to_string(pid_file()) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                println!("Force killing VM (PID {})...", pid);
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        }
    }

    let _ = fs::remove_file(pid_file());
    let _ = fs::remove_file(monitor_socket());

    println!("VM stopped.");
    Ok(())
}

/// Show VM status
pub fn status() -> Result<()> {
    let disk = disk_image();

    if is_running() {
        let pid = fs::read_to_string(pid_file())
            .unwrap_or_default()
            .trim()
            .to_string();
        println!("VM: running (PID {})", pid);

        // Check if SSH is responding
        if try_ssh("true") {
            println!("SSH: ready");

            // Check if bun is installed
            if try_ssh("which bun >/dev/null 2>&1") {
                println!("Bun: installed");
            } else {
                println!("Bun: not yet (cloud-init still running?)");
            }

            // Check if installer is synced
            if try_ssh("test -d ~/installer/src") {
                println!("Installer: synced");
            } else {
                println!("Installer: not synced (run: cargo xtask vm sync)");
            }
        } else {
            println!("SSH: not ready (still booting?)");
        }
    } else {
        println!("VM: not running");
    }

    println!();
    if disk.exists() {
        let meta = fs::metadata(&disk)?;
        println!("Disk: {:.1} GB ({})", meta.len() as f64 / 1e9, disk.display());
    } else {
        println!("Disk: not created (run: cargo xtask vm setup)");
    }

    Ok(())
}

/// SSH into the VM
pub fn ssh() -> Result<()> {
    if !is_running() {
        bail!("VM is not running. Start it with 'cargo xtask vm start --detach'");
    }

    // Wait for SSH if not ready
    if !try_ssh("true") {
        wait_for_ssh(90)?;
    }

    println!("Connecting...");

    SshCommand::new()
        .build()
        .status()
        .context("Failed to SSH")?;

    Ok(())
}

/// Send a command to the VM via SSH
pub fn send(command: &str) -> Result<()> {
    if !is_running() {
        bail!("VM is not running. Start it with 'cargo xtask vm start --detach'");
    }

    // Wait for SSH if not ready
    if !try_ssh("true") {
        wait_for_ssh(90)?;
    }

    let output = run_ssh(command)?;
    print!("{}", output);

    Ok(())
}

/// Sync installer source to VM and install dependencies
pub fn sync() -> Result<()> {
    if !is_running() {
        bail!("VM is not running. Start it with 'cargo xtask vm start --detach'");
    }

    // Wait for SSH if not ready
    if !try_ssh("true") {
        wait_for_ssh(120)?; // Longer timeout for first boot
    }

    // Wait for bun to be installed (cloud-init)
    print!("Waiting for cloud-init to complete");
    std::io::stdout().flush().ok();
    for _ in 0..30 {
        if try_ssh("which bun >/dev/null 2>&1") {
            println!(" done!");
            break;
        }
        print!(".");
        std::io::stdout().flush().ok();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    if !try_ssh("which bun >/dev/null 2>&1") {
        bail!("Bun not installed after waiting. Check: cargo xtask vm ssh");
    }

    println!("Syncing installer source to VM...");

    // Copy installer and docs-content via virtfs
    let sync_script = r#"
set -e
export PATH="$HOME/.bun/bin:$PATH"

mkdir -p ~/installer ~/docs-content

# Copy installer source
rm -rf ~/installer/src ~/installer/bin
cp -r /mnt/share/installer/src ~/installer/
cp -r /mnt/share/installer/bin ~/installer/
cp /mnt/share/installer/package.json ~/installer/
cp /mnt/share/installer/tsconfig.json ~/installer/ 2>/dev/null || true

# Copy docs-content
rm -rf ~/docs-content/src
cp -r /mnt/share/docs-content/src ~/docs-content/
cp /mnt/share/docs-content/package.json ~/docs-content/

# Create workspace if needed
if [ ! -f ~/package.json ]; then
    cat > ~/package.json << 'EOF'
{
  "workspaces": ["installer", "docs-content"]
}
EOF
fi

# Install dependencies
cd ~ && bun install --silent

# Create symlink for root access (installer runs as root)
sudo ln -sf /home/dev/installer/bin/levitate-installer /usr/local/bin/levitate-installer
echo "Sync complete!"
"#;

    let output = run_ssh(sync_script)?;
    print!("{}", output);

    println!("Done! Run 'cargo xtask vm run' to test the installer.");
    Ok(())
}

/// Run the installer in the VM via SSH (interactive)
pub fn run() -> Result<()> {
    if !is_running() {
        bail!("VM is not running. Start it with 'cargo xtask vm start --detach'");
    }

    // Wait for SSH if not ready
    if !try_ssh("true") {
        wait_for_ssh(90)?;
    }

    // Check if installer is synced
    if !try_ssh("test -f ~/installer/bin/levitate-installer") {
        println!("Installer not synced. Running sync first...");
        sync()?;
    }

    // Check if tmux is installed
    if !try_ssh("which tmux >/dev/null 2>&1") {
        bail!("tmux not installed yet (cloud-init still running?).\n  Wait and try again, or check: cargo xtask vm ssh");
    }

    println!("Running installer...");
    println!("  Shift+Tab: switch between shell and docs");
    println!("  exit: quit");
    println!();

    SshCommand::new()
        .force_pty()
        .command("cd ~/installer && TERM=xterm-256color PATH=\"$HOME/.bun/bin:$PATH\" ./bin/levitate-installer")
        .build()
        .status()
        .context("Failed to run installer")?;

    Ok(())
}

// ============================================================================
// ISO-based testing (accurate, matches production)
// ============================================================================

fn kickstarts_dir() -> PathBuf {
    project_root().join("kickstarts")
}

fn iso_path() -> PathBuf {
    kickstarts_dir().join("LevitateOS-1.0-x86_64.iso")
}

/// Build the LevitateOS live ISO
pub fn iso_build(force: bool) -> Result<()> {
    let iso = iso_path();

    if iso.exists() && !force {
        println!("ISO already exists: {:?}", iso);
        println!("Use --force to rebuild.");
        return Ok(());
    }

    // Check for livemedia-creator
    which::which("livemedia-creator")
        .context("livemedia-creator not found.\n  Install: sudo dnf install lorax")?;

    let ks_dir = kickstarts_dir();
    let ks_file = ks_dir.join("levitate-live.ks");

    if !ks_file.exists() {
        bail!("Kickstart not found: {:?}", ks_file);
    }

    println!("=== Building LevitateOS ISO ===");
    println!("This requires sudo and takes several minutes...\n");

    // Clean previous build
    let build_dir = ks_dir.join("build");
    if build_dir.exists() {
        println!("Cleaning previous build...");
        Command::new("sudo")
            .args(["rm", "-rf", &build_dir.display().to_string()])
            .status()?;
    }

    // Run livemedia-creator
    println!("Running livemedia-creator...");
    let status = Command::new("sudo")
        .current_dir(&ks_dir)
        .args([
            "livemedia-creator",
            "--ks", "levitate-live.ks",
            "--no-virt",
            "--resultdir", "build",
            "--project", "LevitateOS",
            "--releasever", "43",
            "--make-iso",
            "--logfile", "build/livemedia.log",
        ])
        .status()
        .context("Failed to run livemedia-creator")?;

    if !status.success() {
        bail!("ISO build failed. Check {:?}/build/livemedia.log", ks_dir);
    }

    // Move ISO to final location
    let built_iso = build_dir.join("images/boot.iso");
    if !built_iso.exists() {
        bail!("ISO not found at expected location: {:?}", built_iso);
    }

    Command::new("sudo")
        .args(["mv", &built_iso.display().to_string(), &iso.display().to_string()])
        .status()?;

    // Fix ownership
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    Command::new("sudo")
        .args(["chown", &format!("{}:{}", user, user), &iso.display().to_string()])
        .status()?;

    println!("\n=== ISO Build Complete ===");
    println!("Output: {:?}", iso);
    println!();
    println!("Test with: cargo xtask vm iso-run");

    Ok(())
}

/// Run the LevitateOS ISO in QEMU
pub fn iso_run(memory: u32, cpus: u32) -> Result<()> {
    check_qemu()?;
    check_kvm()?;

    let iso = iso_path();
    if !iso.exists() {
        bail!("ISO not found: {:?}\n  Build it first: cargo xtask vm iso-build", iso);
    }

    let project = project_root();

    println!("=== LevitateOS ISO Test ===");
    println!("  Memory: {} MB", memory);
    println!("  CPUs: {}", cpus);
    println!("  Shared: {} -> /mnt/share", project.display());
    println!();
    println!("In the VM:");
    println!("  1. Login as root (no password)");
    println!("  2. Wait for setup (installs bun on first boot)");
    println!("  3. Run: levitate-installer");
    println!();
    println!("Press Ctrl+C to stop the VM");
    println!();

    let args = vec![
        "-enable-kvm".to_string(),
        "-cpu".to_string(), "host".to_string(),
        "-m".to_string(), format!("{}M", memory),
        "-smp".to_string(), format!("{}", cpus),
        // Boot from ISO
        "-cdrom".to_string(), iso.display().to_string(),
        "-boot".to_string(), "d".to_string(),
        // Virtfs sharing - mount entire project
        "-virtfs".to_string(),
        format!("local,path={},mount_tag=share,security_model=none,multidevs=remap", project.display()),
        // GUI display
        "-device".to_string(), "virtio-vga-gl".to_string(),
        "-display".to_string(), "gtk,gl=on".to_string(),
        "-device".to_string(), "virtio-keyboard".to_string(),
        "-device".to_string(), "virtio-mouse".to_string(),
        // Serial for debugging
        "-serial".to_string(), format!("file:{}", vm_dir().join("iso-serial.log").display()),
    ];

    let status = Command::new("qemu-system-x86_64")
        .args(&args)
        .status()
        .context("Failed to start QEMU")?;

    if !status.success() {
        bail!("QEMU exited with error");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // SshCommand Builder Tests
    // =========================================================================

    #[test]
    fn ssh_command_default() {
        let args = SshCommand::new().build_args();

        // Should contain base args
        assert!(args.contains(&"-o".to_string()));
        assert!(args.contains(&"StrictHostKeyChecking=no".to_string()));
        assert!(args.contains(&"UserKnownHostsFile=/dev/null".to_string()));
        assert!(args.contains(&"LogLevel=ERROR".to_string()));
        assert!(args.contains(&"IdentitiesOnly=yes".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&SSH_PORT.to_string()));

        // Should have user@host
        assert!(args.contains(&format!("{}@{}", SSH_USER, SSH_HOST)));

        // Should NOT have timeout, BatchMode, -t by default
        assert!(!args.iter().any(|a| a.starts_with("ConnectTimeout=")));
        assert!(!args.contains(&"BatchMode=yes".to_string()));
        assert!(!args.contains(&"-t".to_string()));
    }

    #[test]
    fn ssh_command_with_timeout() {
        let args = SshCommand::new().timeout(5).build_args();
        assert!(args.contains(&"ConnectTimeout=5".to_string()));
    }

    #[test]
    fn ssh_command_with_batch_mode() {
        let args = SshCommand::new().batch_mode().build_args();
        assert!(args.contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn ssh_command_with_force_pty() {
        let args = SshCommand::new().force_pty().build_args();
        assert!(args.contains(&"-t".to_string()));
    }

    #[test]
    fn ssh_command_with_command() {
        let args = SshCommand::new().command("echo hello").build_args();
        assert!(args.last() == Some(&"echo hello".to_string()));
    }

    #[test]
    fn ssh_command_probe_configuration() {
        // Probe: quick timeout, batch mode, simple command
        let args = SshCommand::new()
            .timeout(SSH_TIMEOUT_PROBE)
            .batch_mode()
            .command("true")
            .build_args();

        assert!(args.contains(&format!("ConnectTimeout={}", SSH_TIMEOUT_PROBE)));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.last() == Some(&"true".to_string()));
    }

    #[test]
    fn ssh_command_check_configuration() {
        // Check: medium timeout, batch mode
        let args = SshCommand::new()
            .timeout(SSH_TIMEOUT_CHECK)
            .batch_mode()
            .command("which bun")
            .build_args();

        assert!(args.contains(&format!("ConnectTimeout={}", SSH_TIMEOUT_CHECK)));
        assert!(args.contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn ssh_command_run_configuration() {
        // Run: longer timeout, batch mode
        let args = SshCommand::new()
            .timeout(SSH_TIMEOUT_RUN)
            .batch_mode()
            .command("ls -la")
            .build_args();

        assert!(args.contains(&format!("ConnectTimeout={}", SSH_TIMEOUT_RUN)));
        assert!(args.contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn ssh_command_interactive_with_pty() {
        // Interactive with PTY for tmux
        let args = SshCommand::new()
            .force_pty()
            .command("./installer")
            .build_args();

        assert!(args.contains(&"-t".to_string()));
        assert!(!args.contains(&"BatchMode=yes".to_string()));
    }

    #[test]
    fn ssh_command_argument_order() {
        // Verify user@host comes before command
        let args = SshCommand::new().command("test").build_args();

        let user_host_pos = args.iter().position(|a| a.contains('@')).unwrap();
        let cmd_pos = args.iter().position(|a| a == "test").unwrap();
        assert!(user_host_pos < cmd_pos, "user@host should come before command");
    }

    #[test]
    fn ssh_command_identity_file_included() {
        let args = SshCommand::new().build_args();
        let identity_file = args.iter().find(|a| a.starts_with("IdentityFile="));
        assert!(identity_file.is_some());
        assert!(identity_file.unwrap().contains("id_ed25519"));
    }

    #[test]
    fn ssh_command_all_options_combined() {
        let args = SshCommand::new()
            .timeout(15)
            .batch_mode()
            .force_pty()
            .command("complex command with args")
            .build_args();

        assert!(args.contains(&"ConnectTimeout=15".to_string()));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"-t".to_string()));
        assert!(args.last() == Some(&"complex command with args".to_string()));
    }

    // =========================================================================
    // SSH Constants Tests
    // =========================================================================

    #[test]
    fn ssh_constants_valid() {
        // Timeouts should be reasonable values
        assert!(SSH_TIMEOUT_PROBE > 0 && SSH_TIMEOUT_PROBE <= 5);
        assert!(SSH_TIMEOUT_CHECK > SSH_TIMEOUT_PROBE);
        assert!(SSH_TIMEOUT_RUN > SSH_TIMEOUT_CHECK);

        // Port should be valid
        assert!(SSH_PORT > 1024 && SSH_PORT < 65535);

        // User and host should be non-empty
        assert!(!SSH_USER.is_empty());
        assert!(!SSH_HOST.is_empty());
    }

    #[test]
    fn ssh_timeout_probe_is_fastest() {
        assert!(SSH_TIMEOUT_PROBE < SSH_TIMEOUT_CHECK);
        assert!(SSH_TIMEOUT_PROBE < SSH_TIMEOUT_RUN);
    }

    #[test]
    fn ssh_timeout_hierarchy() {
        // Probe < Check < Run
        assert!(SSH_TIMEOUT_PROBE < SSH_TIMEOUT_CHECK, "Probe timeout should be smaller than check");
        assert!(SSH_TIMEOUT_CHECK < SSH_TIMEOUT_RUN, "Check timeout should be smaller than run");
    }

    // =========================================================================
    // Path Functions Tests (these don't require VM running)
    // =========================================================================

    #[test]
    fn installer_root_is_parent_of_xtask() {
        let root = installer_root();
        assert!(root.ends_with("installer"));
    }

    #[test]
    fn project_root_is_parent_of_installer() {
        let root = project_root();
        let installer = installer_root();
        assert_eq!(installer.parent().unwrap(), root);
    }

    #[test]
    fn vm_dir_is_in_installer() {
        let vm = vm_dir();
        assert!(vm.starts_with(installer_root()));
        assert!(vm.ends_with(".vm"));
    }

    #[test]
    fn disk_image_path() {
        let disk = disk_image();
        assert!(disk.ends_with("fedora-installer-dev.qcow2"));
        assert!(disk.starts_with(vm_dir()));
    }

    #[test]
    fn base_image_path() {
        let base = base_image();
        assert!(base.ends_with("fedora-cloud-base.qcow2"));
        assert!(base.starts_with(vm_dir()));
    }

    #[test]
    fn pid_file_path() {
        let pid = pid_file();
        assert!(pid.ends_with("qemu.pid"));
        assert!(pid.starts_with(vm_dir()));
    }

    #[test]
    fn monitor_socket_path() {
        let socket = monitor_socket();
        assert!(socket.ends_with("qemu-monitor.sock"));
        assert!(socket.starts_with(vm_dir()));
    }

    #[test]
    fn ssh_key_path_is_ed25519() {
        let key = ssh_key_path();
        assert!(key.ends_with("id_ed25519"));
        assert!(key.starts_with(vm_dir()));
    }

    // =========================================================================
    // Fedora Configuration Tests
    // =========================================================================

    #[test]
    fn fedora_version_is_numeric() {
        assert!(FEDORA_VERSION.parse::<u32>().is_ok());
    }

    #[test]
    fn fedora_image_url_contains_version() {
        assert!(FEDORA_IMAGE_URL.contains(FEDORA_VERSION));
    }

    #[test]
    fn fedora_image_url_is_https() {
        assert!(FEDORA_IMAGE_URL.starts_with("https://"));
    }

    #[test]
    fn disk_size_is_valid() {
        // Should be like "20G" or "10G"
        assert!(DISK_SIZE.ends_with('G') || DISK_SIZE.ends_with('M'));
        let numeric_part: String = DISK_SIZE.chars().filter(|c| c.is_numeric()).collect();
        assert!(numeric_part.parse::<u32>().is_ok());
    }
}
