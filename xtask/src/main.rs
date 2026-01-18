mod vm;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Installer development tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// VM management for testing the installer
    Vm {
        #[command(subcommand)]
        action: VmAction,
    },
}

#[derive(Subcommand)]
enum VmAction {
    /// Download and setup Fedora cloud image (for quick dev iteration)
    Setup {
        /// Force re-download even if image exists
        #[arg(long)]
        force: bool,
    },
    /// Start the dev VM (GUI by default)
    Start {
        /// Run headless (no GUI window)
        #[arg(long)]
        headless: bool,
        /// Memory in MB
        #[arg(long, default_value = "4096")]
        memory: u32,
        /// Number of CPUs
        #[arg(long, default_value = "2")]
        cpus: u32,
    },
    /// Stop the dev VM
    Stop,
    /// Reset dev VM disk to clean state (keeps base image)
    Reset,
    /// Show dev VM status
    Status,
    /// SSH into the dev VM
    Ssh,
    /// Run a command in the dev VM
    Send {
        /// Command to run
        command: String,
    },
    /// Sync installer source to dev VM and install deps
    Sync,
    /// Run the installer in the dev VM
    Run,
    /// Build the LevitateOS live ISO (accurate testing)
    IsoBuild {
        /// Force rebuild even if ISO exists
        #[arg(long)]
        force: bool,
    },
    /// Run the LevitateOS ISO in QEMU (accurate testing)
    IsoRun {
        /// Memory in MB
        #[arg(long, default_value = "4096")]
        memory: u32,
        /// Number of CPUs
        #[arg(long, default_value = "2")]
        cpus: u32,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Vm { action } => match action {
            VmAction::Setup { force } => vm::setup(force),
            VmAction::Start { headless, memory, cpus } => vm::start(!headless, memory, cpus),
            VmAction::Stop => vm::stop(),
            VmAction::Reset => vm::reset(),
            VmAction::Status => vm::status(),
            VmAction::Ssh => vm::ssh(),
            VmAction::Send { command } => vm::send(&command),
            VmAction::Sync => vm::sync(),
            VmAction::Run => vm::run(),
            VmAction::IsoBuild { force } => vm::iso_build(force),
            VmAction::IsoRun { memory, cpus } => vm::iso_run(memory, cpus),
        },
    }
}
