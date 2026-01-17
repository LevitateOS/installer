//! Convert conversation templates to training snapshots.
//!
//! Reads: data/conversations/*.jsonl (templates with placeholders)
//! Writes: data/training/augmented_dataset.jsonl (training snapshots)
//!
//! Each template is expanded into multiple variations based on:
//! - Disk types (SATA, NVMe, VirtIO)
//! - Boot modes (UEFI, Legacy BIOS)
//! - Single vs multi-disk configurations
//! - Various user inputs (hostname, username, timezone)

use rand::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

// =============================================================================
// System Context Variations
// =============================================================================

struct DiskConfig {
    device: &'static str,
    size: &'static str,
    model: &'static str,
    type_name: &'static str,
    type_explanation: &'static str,
}

const DISK_CONFIGS: &[DiskConfig] = &[
    DiskConfig { device: "sda", size: "256G", model: "Samsung 860 EVO", type_name: "SATA SSD", type_explanation: "SATA is a common interface for SSDs and hard drives. Your drive connects via a SATA cable." },
    DiskConfig { device: "sda", size: "500G", model: "Samsung 870 EVO", type_name: "SATA SSD", type_explanation: "SATA is a common interface for SSDs and hard drives. Your drive connects via a SATA cable." },
    DiskConfig { device: "sda", size: "1T", model: "Crucial MX500", type_name: "SATA SSD", type_explanation: "SATA is a common interface for SSDs and hard drives. Your drive connects via a SATA cable." },
    DiskConfig { device: "sda", size: "500G", model: "Seagate Barracuda", type_name: "SATA HDD", type_explanation: "SATA is a common interface for hard drives. This is a spinning disk drive - reliable but slower than SSDs." },
    DiskConfig { device: "nvme0n1", size: "256G", model: "WD Blue SN570", type_name: "NVMe SSD", type_explanation: "NVMe is a fast storage interface. Your drive plugs directly into the motherboard and is much faster than SATA drives." },
    DiskConfig { device: "nvme0n1", size: "500G", model: "Samsung 980", type_name: "NVMe SSD", type_explanation: "NVMe is a fast storage interface. Your drive plugs directly into the motherboard and is much faster than SATA drives." },
    DiskConfig { device: "nvme0n1", size: "1T", model: "Samsung 990 PRO", type_name: "NVMe SSD", type_explanation: "NVMe is a high-performance storage interface. This is a top-tier drive - very fast!" },
    DiskConfig { device: "nvme0n1", size: "2T", model: "WD Black SN850X", type_name: "NVMe SSD", type_explanation: "NVMe is a high-performance storage interface. This is a top-tier drive with lots of space!" },
    DiskConfig { device: "vda", size: "20G", model: "VirtIO Block Device", type_name: "virtual drive", type_explanation: "VirtIO is a virtualized storage interface - you're running in a virtual machine. It works just like a regular drive." },
    DiskConfig { device: "vda", size: "40G", model: "VirtIO Block Device", type_name: "virtual drive", type_explanation: "VirtIO is a virtualized storage interface - you're running in a virtual machine. It works just like a regular drive." },
    DiskConfig { device: "vda", size: "100G", model: "VirtIO Block Device", type_name: "virtual drive", type_explanation: "VirtIO is a virtualized storage interface - you're running in a virtual machine. It works just like a regular drive." },
];

struct SecondaryDiskConfig {
    device: &'static str,
    size: &'static str,
    model: &'static str,
}

const SECONDARY_DISK_CONFIGS: &[SecondaryDiskConfig] = &[
    SecondaryDiskConfig { device: "sdb", size: "1T", model: "WD Blue HDD" },
    SecondaryDiskConfig { device: "sdb", size: "2T", model: "Seagate Barracuda" },
    SecondaryDiskConfig { device: "nvme1n1", size: "500G", model: "Crucial P3" },
    SecondaryDiskConfig { device: "sdb", size: "500G", model: "Kingston A400" },
];

const BOOT_MODES: &[&str] = &["UEFI", "Legacy BIOS"];

const HOSTNAMES: &[&str] = &["mypc", "laptop", "desktop", "workstation", "devbox", "homepc", "linuxbox", "levitate-pc"];
const USERNAMES: &[&str] = &["user", "admin", "dev", "me", "main"];

const TIMEZONES: &[(&str, &str)] = &[
    ("new york", "America/New_York"),
    ("los angeles", "America/Los_Angeles"),
    ("chicago", "America/Chicago"),
    ("london", "Europe/London"),
    ("berlin", "Europe/Berlin"),
    ("tokyo", "Asia/Tokyo"),
    ("sydney", "Australia/Sydney"),
    ("utc", "UTC"),
];

struct Filesystem {
    name: &'static str,
    explanation: &'static str,
    tip: &'static str,
}

const FILESYSTEMS: &[Filesystem] = &[
    Filesystem { name: "ext4", explanation: "ext4 is the default Linux filesystem - reliable, fast, and well-tested. Great for most users.", tip: "" },
    Filesystem { name: "btrfs", explanation: "btrfs supports snapshots, compression, and copy-on-write. Great for advanced users who want easy backups.", tip: "Look into 'snapper' or 'timeshift' for btrfs snapshots." },
    Filesystem { name: "xfs", explanation: "xfs is excellent for large files and high-performance workloads. Used by many servers.", tip: "" },
];

// =============================================================================
// Partition naming helpers
// =============================================================================

fn get_partition_suffix(disk: &str, part_num: u8) -> String {
    if disk.starts_with("nvme") || disk.starts_with("mmcblk") {
        format!("{}p{}", disk, part_num)
    } else {
        format!("{}{}", disk, part_num)
    }
}

fn get_boot_mount_path(boot_mode: &str) -> &'static str {
    if boot_mode == "UEFI" {
        "boot/efi"
    } else {
        "boot"
    }
}

fn get_partition_cmd(disk: &str, boot_mode: &str) -> String {
    if boot_mode == "UEFI" {
        format!("sgdisk -Z /dev/{} && sgdisk -n 1:0:+512M -t 1:ef00 -n 2:0:0 -t 2:8300 /dev/{}", disk, disk)
    } else {
        format!("parted -s /dev/{} mklabel msdos mkpart primary ext4 1MiB 512MiB mkpart primary ext4 512MiB 100%", disk)
    }
}

fn get_format_cmd(boot_part: &str, root_part: &str, boot_mode: &str, fs: &str) -> String {
    if boot_mode == "UEFI" {
        format!("mkfs.fat -F32 /dev/{} && mkfs.{} /dev/{}", boot_part, fs, root_part)
    } else {
        format!("mkfs.ext4 /dev/{} && mkfs.{} /dev/{}", boot_part, fs, root_part)
    }
}

fn get_bootloader_cmd(disk: &str, boot_mode: &str) -> String {
    if boot_mode == "UEFI" {
        "arch-chroot /mnt bootctl install".to_string()
    } else {
        format!("arch-chroot /mnt grub-install --target=i386-pc /dev/{} && arch-chroot /mnt grub-mkconfig -o /boot/grub/grub.cfg", disk)
    }
}

// =============================================================================
// System State Tracking
// =============================================================================

#[derive(Clone)]
struct DiskPartition {
    name: String,
    size: String,
    fstype: Option<String>,
    mountpoint: Option<String>,
}

#[derive(Clone)]
struct Disk {
    device: String,
    size: String,
    model: String,
    partitions: Vec<DiskPartition>,
}

#[derive(Clone)]
struct SystemState {
    boot_mode: String,
    network: String,
    hostname: String,
    timezone: String,
    disks: Vec<Disk>,
    users: Vec<String>,
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            boot_mode: "UEFI".to_string(),
            network: "Connected".to_string(),
            hostname: "archiso".to_string(),
            timezone: "not set".to_string(),
            disks: Vec::new(),
            users: Vec::new(),
        }
    }
}

impl SystemState {
    fn to_context(&self) -> String {
        let mut lines = vec!["## Current System State".to_string(), String::new()];
        lines.push(format!("- Boot mode: {}", self.boot_mode));
        lines.push(format!("- Network: {}", self.network));
        lines.push(format!("- Hostname: {}", self.hostname));
        lines.push(format!("- Timezone: {}", self.timezone));
        lines.push(String::new());
        lines.push("## Available Disks".to_string());
        lines.push(String::new());

        for disk in &self.disks {
            lines.push(format!("- {}: {} ({})", disk.device, disk.size, disk.model));
            for part in &disk.partitions {
                let fs_str = part.fstype.as_ref().map(|f| format!(" [{}]", f)).unwrap_or_default();
                let mount_str = part.mountpoint.as_ref().map(|m| format!(" mounted at {}", m)).unwrap_or_default();
                lines.push(format!("  - /dev/{}: {}{}{}", part.name, part.size, fs_str, mount_str));
            }
        }

        // Check for mounts
        let has_mounts = self.disks.iter().any(|d| d.partitions.iter().any(|p| p.mountpoint.is_some()));
        if has_mounts {
            lines.push(String::new());
            lines.push("## Current Mounts".to_string());
            for disk in &self.disks {
                for part in &disk.partitions {
                    if let Some(mp) = &part.mountpoint {
                        lines.push(format!("- /dev/{} on {}", part.name, mp));
                    }
                }
            }
        }

        if !self.users.is_empty() {
            lines.push(String::new());
            lines.push(format!("## Existing Users: {}", self.users.join(", ")));
        }

        lines.join("\n")
    }

    fn apply_command(&mut self, command: &str) {
        let dev_re = Regex::new(r"/dev/(sd[a-z]|nvme\d+n\d+|vd[a-z])").unwrap();
        let part_re = Regex::new(r"-n\s*(\d+):([^:]*):([^\s]+)").unwrap();

        // Handle sgdisk/parted partitioning
        if command.contains("sgdisk") || command.contains("parted") {
            if let Some(cap) = dev_re.captures(command) {
                let device_name = cap.get(1).unwrap().as_str();
                let device = format!("/dev/{}", device_name);

                if let Some(disk) = self.disks.iter_mut().find(|d| d.device == device) {
                    if command.contains("-Z") || command.contains("mklabel") {
                        disk.partitions.clear();
                    }

                    for cap in part_re.captures_iter(command) {
                        let part_num: u8 = cap.get(1).unwrap().as_str().parse().unwrap_or(1);
                        let end = cap.get(3).unwrap().as_str();

                        let part_name = get_partition_suffix(device_name, part_num);
                        let size = if end.starts_with('+') {
                            end[1..].to_string()
                        } else if end == "0" {
                            "remaining".to_string()
                        } else {
                            end.to_string()
                        };

                        disk.partitions.push(DiskPartition {
                            name: part_name,
                            size,
                            fstype: None,
                            mountpoint: None,
                        });
                    }

                    // Handle parted mkpart
                    if command.contains("mkpart") && disk.partitions.is_empty() {
                        disk.partitions = vec![
                            DiskPartition {
                                name: get_partition_suffix(device_name, 1),
                                size: "512M".to_string(),
                                fstype: None,
                                mountpoint: None,
                            },
                            DiskPartition {
                                name: get_partition_suffix(device_name, 2),
                                size: "remaining".to_string(),
                                fstype: None,
                                mountpoint: None,
                            },
                        ];
                    }
                }
            }
        }

        // Handle mkfs
        if command.contains("mkfs") {
            let fat_re = Regex::new(r"mkfs\.fat[^\s]*\s+/dev/(\S+)").unwrap();
            let ext4_re = Regex::new(r"mkfs\.ext4\s+/dev/(\S+)").unwrap();
            let btrfs_re = Regex::new(r"mkfs\.btrfs\s+/dev/(\S+)").unwrap();
            let xfs_re = Regex::new(r"mkfs\.xfs\s+/dev/(\S+)").unwrap();

            for cap in fat_re.captures_iter(command) {
                self.set_fstype(cap.get(1).unwrap().as_str(), "vfat");
            }
            for cap in ext4_re.captures_iter(command) {
                self.set_fstype(cap.get(1).unwrap().as_str(), "ext4");
            }
            for cap in btrfs_re.captures_iter(command) {
                self.set_fstype(cap.get(1).unwrap().as_str(), "btrfs");
            }
            for cap in xfs_re.captures_iter(command) {
                self.set_fstype(cap.get(1).unwrap().as_str(), "xfs");
            }
        }

        // Handle mount
        let mount_re = Regex::new(r"mount\s+/dev/(\S+)\s+(/\S+)").unwrap();
        for cap in mount_re.captures_iter(command) {
            self.set_mountpoint(cap.get(1).unwrap().as_str(), cap.get(2).unwrap().as_str());
        }

        // Handle umount
        if command.contains("umount") {
            for disk in &mut self.disks {
                for part in &mut disk.partitions {
                    part.mountpoint = None;
                }
            }
        }

        // Handle hostname
        let hostname_re = Regex::new(r#"echo\s+['"]([^'"]+)['"]\s*>\s*/mnt/etc/hostname"#).unwrap();
        if let Some(cap) = hostname_re.captures(command) {
            self.hostname = cap.get(1).unwrap().as_str().to_string();
        }

        // Handle timezone
        let tz_re = Regex::new(r"/usr/share/zoneinfo/(\S+)\s+/mnt/etc/localtime").unwrap();
        if let Some(cap) = tz_re.captures(command) {
            self.timezone = cap.get(1).unwrap().as_str().to_string();
        }

        // Handle useradd
        let user_re = Regex::new(r"useradd\s+.*\s+(\w+)\s*$").unwrap();
        if let Some(cap) = user_re.captures(command) {
            let user = cap.get(1).unwrap().as_str().to_string();
            if !self.users.contains(&user) {
                self.users.push(user);
            }
        }
    }

    fn set_fstype(&mut self, part_name: &str, fstype: &str) {
        for disk in &mut self.disks {
            for part in &mut disk.partitions {
                if part.name == part_name {
                    part.fstype = Some(fstype.to_string());
                }
            }
        }
    }

    fn set_mountpoint(&mut self, part_name: &str, mountpoint: &str) {
        for disk in &mut self.disks {
            for part in &mut disk.partitions {
                if part.name == part_name {
                    part.mountpoint = Some(mountpoint.to_string());
                }
            }
        }
    }
}

// =============================================================================
// Template Types
// =============================================================================

#[derive(Deserialize, Serialize, Clone)]
struct Turn {
    user: Option<String>,
    #[serde(rename = "type")]
    turn_type: Option<String>,
    command: Option<String>,
    response: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct Template {
    id: Option<String>,
    desc: Option<String>,
    turns: Vec<Turn>,
    system_context: Option<String>,  // For legacy format
    #[serde(default)]
    _legacy: bool,
}

#[derive(Serialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ExpectedResponse {
    #[serde(rename = "type")]
    response_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<String>,
}

#[derive(Serialize)]
struct Snapshot {
    system_context: String,
    messages: Vec<Message>,
    expected_response: ExpectedResponse,
}

// =============================================================================
// Template Expansion
// =============================================================================

fn fill_placeholders(text: &str, ctx: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (key, value) in ctx {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

fn parse_size(s: &str) -> f64 {
    if s.ends_with('T') {
        s[..s.len()-1].parse::<f64>().unwrap_or(0.0) * 1000.0
    } else if s.ends_with('G') {
        s[..s.len()-1].parse::<f64>().unwrap_or(0.0)
    } else {
        0.0
    }
}

fn generate_variations<R: Rng>(template: &Template, rng: &mut R) -> Vec<HashMap<String, String>> {
    let template_json = serde_json::to_string(template).unwrap_or_default();
    let needs_secondary = template_json.contains("{SECONDARY_DISK}") || template_json.contains("{BIGGER");
    let needs_filesystem = template_json.contains("{REQUESTED_FS}");

    let mut variations = Vec::new();
    let num_disk_samples = 2;

    // Sample disk configs
    let mut disk_indices: Vec<usize> = (0..DISK_CONFIGS.len()).collect();
    disk_indices.shuffle(rng);
    let disk_samples: Vec<&DiskConfig> = disk_indices.iter()
        .take(num_disk_samples.min(DISK_CONFIGS.len()))
        .map(|&i| &DISK_CONFIGS[i])
        .collect();

    for disk_config in disk_samples {
        for &boot_mode in BOOT_MODES {
            // Skip BIOS with NVMe (rare/unrealistic)
            if boot_mode == "Legacy BIOS" && disk_config.device.starts_with("nvme") {
                continue;
            }

            let boot_part = get_partition_suffix(disk_config.device, 1);
            let root_part = get_partition_suffix(disk_config.device, 2);

            let mut ctx = HashMap::new();
            ctx.insert("PRIMARY_DISK".to_string(), disk_config.device.to_string());
            ctx.insert("DISK_SIZE".to_string(), disk_config.size.to_string());
            ctx.insert("DISK_MODEL".to_string(), disk_config.model.to_string());
            ctx.insert("DISK_TYPE_NAME".to_string(), disk_config.type_name.to_string());
            ctx.insert("DISK_TYPE_EXPLANATION".to_string(), disk_config.type_explanation.to_string());
            ctx.insert("BOOT_MODE".to_string(), boot_mode.to_string());
            ctx.insert("BOOT_PARTITION".to_string(), boot_part.clone());
            ctx.insert("ROOT_PARTITION".to_string(), root_part.clone());
            ctx.insert("BOOT_MOUNT_PATH".to_string(), get_boot_mount_path(boot_mode).to_string());
            ctx.insert("PARTITION_DISK_CMD".to_string(), get_partition_cmd(disk_config.device, boot_mode));
            ctx.insert("FORMAT_CMD".to_string(), get_format_cmd(&boot_part, &root_part, boot_mode, "ext4"));
            ctx.insert("BOOTLOADER_INSTALL".to_string(), get_bootloader_cmd(disk_config.device, boot_mode));
            ctx.insert("PARTITION_LAYOUT_DESC".to_string(), if boot_mode == "UEFI" { "EFI and root partitions" } else { "boot and root partitions" }.to_string());

            // User inputs
            let hostname = *HOSTNAMES.choose(rng).unwrap();
            let username = *USERNAMES.choose(rng).unwrap();
            let (tz_input, tz_path) = *TIMEZONES.choose(rng).unwrap();

            ctx.insert("HOSTNAME".to_string(), hostname.to_string());
            let alt_hostname = *HOSTNAMES.iter().filter(|&&h| h != hostname).choose(rng).unwrap_or(&hostname);
            ctx.insert("HOSTNAME_ALT".to_string(), alt_hostname.to_string());
            ctx.insert("USERNAME".to_string(), username.to_string());
            ctx.insert("TIMEZONE".to_string(), tz_path.to_string());
            ctx.insert("TIMEZONE_INPUT".to_string(), tz_input.to_string());

            // Secondary disk if needed
            if needs_secondary {
                let sec = SECONDARY_DISK_CONFIGS.choose(rng).unwrap();
                let sec_boot_part = get_partition_suffix(sec.device, 1);
                let sec_root_part = get_partition_suffix(sec.device, 2);

                let (bigger_disk, bigger_size, bigger_boot, bigger_root) = if parse_size(sec.size) > parse_size(disk_config.size) {
                    (sec.device, sec.size, sec_boot_part.clone(), sec_root_part.clone())
                } else {
                    (disk_config.device, disk_config.size, boot_part.clone(), root_part.clone())
                };

                ctx.insert("SECONDARY_DISK".to_string(), sec.device.to_string());
                ctx.insert("SECONDARY_SIZE".to_string(), sec.size.to_string());
                ctx.insert("SECONDARY_MODEL".to_string(), sec.model.to_string());
                ctx.insert("BIGGER_DISK".to_string(), bigger_disk.to_string());
                ctx.insert("BIGGER_SIZE".to_string(), bigger_size.to_string());
                ctx.insert("BIGGER_BOOT_PART".to_string(), bigger_boot);
                ctx.insert("BIGGER_ROOT_PART".to_string(), bigger_root);
                ctx.insert("PARTITION_BIGGER_DISK_CMD".to_string(), get_partition_cmd(bigger_disk, boot_mode));
                ctx.insert("FORMAT_BIGGER_DISK_CMD".to_string(), get_format_cmd(&get_partition_suffix(bigger_disk, 1), &get_partition_suffix(bigger_disk, 2), boot_mode, "ext4"));
                ctx.insert("MULTI_DISK_ADVICE".to_string(), format!("The {} ({}) and the {} ({}). SSDs are faster for the OS, HDDs are better for bulk storage.", sec.model, sec.size, disk_config.model, disk_config.size));
                ctx.insert("USER_DISK_CHOICE".to_string(), if disk_config.model.contains("SSD") { "SSD" } else { "faster one" }.to_string());
                ctx.insert("INITIAL_CHOICE".to_string(), "big one".to_string());
                ctx.insert("CHANGED_CHOICE".to_string(), if disk_config.model.contains("SSD") { "SSD" } else { "smaller one" }.to_string());
                ctx.insert("INITIAL_CHOICE_RESPONSE".to_string(), format!("The {} drive? The SSD would be faster for the OS though.", sec.size));
            }

            // Filesystem variations
            if needs_filesystem {
                for fs in FILESYSTEMS {
                    let mut fs_ctx = ctx.clone();
                    fs_ctx.insert("REQUESTED_FS".to_string(), fs.name.to_string());
                    fs_ctx.insert("FS_EXPLANATION".to_string(), fs.explanation.to_string());
                    fs_ctx.insert("FS_POST_INSTALL_TIP".to_string(), fs.tip.to_string());
                    fs_ctx.insert("FORMAT_CMD_CUSTOM_FS".to_string(), get_format_cmd(&boot_part, &root_part, boot_mode, fs.name));
                    variations.push(fs_ctx);
                }
            } else {
                variations.push(ctx);
            }
        }
    }

    // Add context-specific placeholders
    for ctx in &mut variations {
        ctx.insert("NEXT_STEP_SUGGESTION".to_string(), "partition and format the disk".to_string());
    }

    variations
}

fn convert_template_with_context(template: &Template, ctx: &HashMap<String, String>) -> Vec<Snapshot> {
    let mut snapshots = Vec::new();

    let mut state = SystemState::default();
    state.boot_mode = ctx.get("BOOT_MODE").cloned().unwrap_or_else(|| "UEFI".to_string());

    state.disks.push(Disk {
        device: format!("/dev/{}", ctx.get("PRIMARY_DISK").map(|s| s.as_str()).unwrap_or("sda")),
        size: ctx.get("DISK_SIZE").cloned().unwrap_or_default(),
        model: ctx.get("DISK_MODEL").cloned().unwrap_or_default(),
        partitions: Vec::new(),
    });

    // Add secondary disk if present
    if let Some(sec_disk) = ctx.get("SECONDARY_DISK") {
        state.disks.push(Disk {
            device: format!("/dev/{}", sec_disk),
            size: ctx.get("SECONDARY_SIZE").cloned().unwrap_or_default(),
            model: ctx.get("SECONDARY_MODEL").cloned().unwrap_or_default(),
            partitions: Vec::new(),
        });
    }

    let mut messages: Vec<Message> = Vec::new();

    for turn in &template.turns {
        let user_content = fill_placeholders(turn.user.as_deref().unwrap_or(""), ctx);
        let response_type = turn.turn_type.as_deref().unwrap_or("text");
        let command = fill_placeholders(turn.command.as_deref().unwrap_or(""), ctx);
        let response_text = fill_placeholders(turn.response.as_deref().unwrap_or(""), ctx);

        messages.push(Message {
            role: "user".to_string(),
            content: user_content,
        });

        let expected = if response_type == "command" {
            ExpectedResponse {
                response_type: "command".to_string(),
                command: Some(command.clone()),
                response: None,
            }
        } else {
            ExpectedResponse {
                response_type: "text".to_string(),
                command: None,
                response: Some(response_text.clone()),
            }
        };

        snapshots.push(Snapshot {
            system_context: state.to_context(),
            messages: messages.clone(),
            expected_response: expected,
        });

        // Generate assistant message for history
        let assistant_content = if response_type == "command" {
            state.apply_command(&command);
            format!("$ {}", command)
        } else {
            response_text
        };

        messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_content,
        });
    }

    snapshots
}

fn generate_truncated_templates(template: &Template, skip_if_short: usize, max_truncations: usize) -> Vec<Template> {
    let n = template.turns.len();

    if n <= skip_if_short {
        return vec![template.clone()];
    }

    let mut lengths = if n <= 6 {
        vec![1, n]
    } else {
        vec![1, n / 2, n]
    };

    lengths.truncate(max_truncations);

    lengths.into_iter().map(|length| {
        Template {
            id: template.id.as_ref().map(|id| format!("{}_t{}", id, length)),
            desc: template.desc.as_ref().map(|d| format!("{} (truncated to {} turns)", d, length)),
            turns: template.turns[..length].to_vec(),
            system_context: None,
            _legacy: template._legacy,
        }
    }).collect()
}

fn convert_legacy_conversation(conv: &Template) -> Option<Template> {
    let ctx = conv.system_context.as_ref()?;

    // Find primary disk: "- /dev/sda: 500G (Samsung SSD 870)"
    let disk_re = Regex::new(r"-\s*/dev/(sd[a-z]|nvme\d+n\d+|vd[a-z]):\s*(\S+)\s*\(([^)]+)\)").unwrap();
    let cap = disk_re.captures(ctx)?;

    let primary_disk = cap.get(1).unwrap().as_str().to_string();
    let _disk_size = cap.get(2).unwrap().as_str().to_string();
    let _disk_model = cap.get(3).unwrap().as_str().to_string();

    let (boot_part, root_part) = if primary_disk.starts_with("nvme") || primary_disk.starts_with("mmcblk") {
        (format!("{}p1", primary_disk), format!("{}p2", primary_disk))
    } else {
        (format!("{}1", primary_disk), format!("{}2", primary_disk))
    };

    // Check for secondary disk
    let all_disks: Vec<_> = disk_re.captures_iter(ctx).collect();
    let has_secondary = all_disks.len() > 1;

    let mut replacements = HashMap::new();
    replacements.insert(format!("/dev/{}", primary_disk), "/dev/{PRIMARY_DISK}".to_string());
    replacements.insert(format!("/dev/{}", boot_part), "/dev/{BOOT_PARTITION}".to_string());
    replacements.insert(format!("/dev/{}", root_part), "/dev/{ROOT_PARTITION}".to_string());

    if has_secondary {
        let sec_cap = &all_disks[1];
        let sec_disk = sec_cap.get(1).unwrap().as_str();
        let (sec_boot, sec_root) = if sec_disk.starts_with("nvme") || sec_disk.starts_with("mmcblk") {
            (format!("{}p1", sec_disk), format!("{}p2", sec_disk))
        } else {
            (format!("{}1", sec_disk), format!("{}2", sec_disk))
        };
        replacements.insert(format!("/dev/{}", sec_disk), "/dev/{SECONDARY_DISK}".to_string());
        replacements.insert(format!("/dev/{}", sec_boot), "/dev/{SECONDARY_BOOT}".to_string());
        replacements.insert(format!("/dev/{}", sec_root), "/dev/{SECONDARY_ROOT}".to_string());
    }

    fn apply_replacements(text: &str, replacements: &HashMap<String, String>) -> String {
        let mut sorted: Vec<_> = replacements.iter().collect();
        sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        let mut result = text.to_string();
        for (old, new) in sorted {
            result = result.replace(old, new);
        }
        result
    }

    let new_turns: Vec<Turn> = conv.turns.iter().map(|turn| {
        Turn {
            user: turn.user.clone(),
            turn_type: turn.turn_type.clone(),
            command: turn.command.as_ref().map(|c| apply_replacements(c, &replacements)),
            response: turn.response.as_ref().map(|r| apply_replacements(r, &replacements)),
        }
    }).collect();

    Some(Template {
        id: conv.id.clone(),
        desc: conv.desc.clone(),
        turns: new_turns,
        system_context: None,
        _legacy: true,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = StdRng::seed_from_u64(42);

    // Determine paths
    let exe_dir = std::env::current_exe()?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Try to find data directory
    let data_dir = if PathBuf::from("data/conversations").exists() {
        PathBuf::from("data")
    } else if exe_dir.join("../data/conversations").exists() {
        exe_dir.join("../data")
    } else if exe_dir.join("../../data/conversations").exists() {
        exe_dir.join("../../data")
    } else {
        eprintln!("Error: Cannot find data/conversations directory");
        eprintln!("Run from installer directory or set current directory correctly");
        std::process::exit(1);
    };

    let conv_dir = data_dir.join("conversations");
    let output_file = data_dir.join("training/augmented_dataset.jsonl");
    let test_output_file = data_dir.join("testing/test_dataset.jsonl");

    const TEST_SPLIT: f64 = 0.15;

    // Load all conversation files
    let mut templates = Vec::new();
    let mut legacy_count = 0;

    for entry in fs::read_dir(&conv_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
            println!("Loading {}...", path.file_name().unwrap().to_string_lossy());

            let file = File::open(&path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<Template>(&line) {
                    Ok(conv) => {
                        // Check if legacy format
                        if let Some(ctx) = &conv.system_context {
                            if ctx.contains("/dev/sd") || ctx.contains("/dev/nvme") || ctx.contains("/dev/vd") {
                                if let Some(converted) = convert_legacy_conversation(&conv) {
                                    templates.push(converted);
                                    legacy_count += 1;
                                }
                                continue;
                            }
                        }
                        templates.push(conv);
                    }
                    Err(e) => {
                        eprintln!("  WARNING: Invalid JSON: {}", e);
                    }
                }
            }
        }
    }

    println!("\nLoaded {} templates ({} converted from legacy format)", templates.len(), legacy_count);

    // Split templates
    templates.shuffle(&mut rng);
    let split_idx = ((templates.len() as f64) * (1.0 - TEST_SPLIT)) as usize;
    let (train_templates, test_templates) = templates.split_at(split_idx);

    println!("Split: {} train, {} test ({:.0}% held out)", train_templates.len(), test_templates.len(), TEST_SPLIT * 100.0);

    // Generate truncated versions
    let expanded_train: Vec<Template> = train_templates.iter()
        .flat_map(|t| generate_truncated_templates(t, 3, 3))
        .collect();

    let expanded_test: Vec<Template> = test_templates.iter()
        .flat_map(|t| generate_truncated_templates(t, 3, 3))
        .collect();

    println!("Expanded: {} train templates, {} test templates", expanded_train.len(), expanded_test.len());

    // Generate training snapshots
    let mut train_snapshots = Vec::new();
    for template in &expanded_train {
        let variations = generate_variations(template, &mut rng);
        for ctx in variations {
            let snapshots = convert_template_with_context(template, &ctx);
            train_snapshots.extend(snapshots);
        }
    }

    // Generate test snapshots with different seed
    let mut test_rng = StdRng::seed_from_u64(123);
    let mut test_snapshots = Vec::new();
    for template in &expanded_test {
        let variations = generate_variations(template, &mut test_rng);
        for ctx in variations {
            let snapshots = convert_template_with_context(template, &ctx);
            test_snapshots.extend(snapshots);
        }
    }

    // Write training output
    fs::create_dir_all(output_file.parent().unwrap())?;
    let train_file = File::create(&output_file)?;
    let mut train_writer = BufWriter::new(train_file);
    for snapshot in &train_snapshots {
        serde_json::to_writer(&mut train_writer, snapshot)?;
        writeln!(train_writer)?;
    }

    // Write test output
    fs::create_dir_all(test_output_file.parent().unwrap())?;
    let test_file = File::create(&test_output_file)?;
    let mut test_writer = BufWriter::new(test_file);
    for snapshot in &test_snapshots {
        serde_json::to_writer(&mut test_writer, snapshot)?;
        writeln!(test_writer)?;
    }

    // Print stats
    println!("\n{}", "=".repeat(60));
    println!("TRAINING SET: {} snapshots", train_snapshots.len());
    println!("Output: {}", output_file.display());

    let text_count = train_snapshots.iter().filter(|s| s.expected_response.response_type == "text").count();
    let cmd_count = train_snapshots.iter().filter(|s| s.expected_response.response_type == "command").count();
    println!("  Response types: Text {} ({:.1}%), Command {} ({:.1}%)",
        text_count, 100.0 * text_count as f64 / train_snapshots.len() as f64,
        cmd_count, 100.0 * cmd_count as f64 / train_snapshots.len() as f64);

    println!("\n{}", "=".repeat(60));
    println!("TEST SET: {} snapshots (held out, unseen)", test_snapshots.len());
    println!("Output: {}", test_output_file.display());

    let text_count = test_snapshots.iter().filter(|s| s.expected_response.response_type == "text").count();
    let cmd_count = test_snapshots.iter().filter(|s| s.expected_response.response_type == "command").count();
    println!("  Response types: Text {} ({:.1}%), Command {} ({:.1}%)",
        text_count, 100.0 * text_count as f64 / test_snapshots.len() as f64,
        cmd_count, 100.0 * cmd_count as f64 / test_snapshots.len() as f64);

    // Combined stats
    let all_snapshots: Vec<_> = train_snapshots.iter().chain(test_snapshots.iter()).collect();
    println!("\n{}", "=".repeat(60));
    println!("COMBINED STATS");

    let lengths: Vec<_> = all_snapshots.iter().map(|s| s.messages.len()).collect();
    let short = lengths.iter().filter(|&&l| l <= 2).count();
    let medium = lengths.iter().filter(|&&l| l >= 3 && l <= 6).count();
    let long = lengths.iter().filter(|&&l| l >= 7 && l <= 12).count();
    let very_long = lengths.iter().filter(|&&l| l > 12).count();

    println!("\nConversation lengths:");
    println!("  1-2 messages: {} ({:.1}%)", short, 100.0 * short as f64 / all_snapshots.len() as f64);
    println!("  3-6 messages: {} ({:.1}%)", medium, 100.0 * medium as f64 / all_snapshots.len() as f64);
    println!("  7-12 messages: {} ({:.1}%)", long, 100.0 * long as f64 / all_snapshots.len() as f64);
    println!("  13+ messages: {} ({:.1}%)", very_long, 100.0 * very_long as f64 / all_snapshots.len() as f64);

    let uefi_count = all_snapshots.iter().filter(|s| s.system_context.contains("UEFI")).count();
    let bios_count = all_snapshots.iter().filter(|s| s.system_context.contains("Legacy BIOS")).count();
    println!("\nBoot modes:");
    println!("  UEFI: {} ({:.1}%)", uefi_count, 100.0 * uefi_count as f64 / all_snapshots.len() as f64);
    println!("  Legacy BIOS: {} ({:.1}%)", bios_count, 100.0 * bios_count as f64 / all_snapshots.len() as f64);

    Ok(())
}
