//! p64tool - Linux programming tool for the Retevis MateTalk P64 / P4 DMR radio.
//!
//! Currently READ-ONLY: it dumps the radio's codeplug (memory image) so we can
//! decode the field layout. Writing back to the radio is deliberately not
//! implemented yet - a bad write could brick the radio.

mod codeplug;
mod config;
mod identity;
mod proto;
mod regs;
mod serial;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "p64tool",
    version,
    about = "Retevis MateTalk P64 / P4 programmer (Linux)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read the whole codeplug from the radio into a directory of raw region dumps.
    Read {
        /// Serial port, e.g. /dev/ttyUSB0
        #[arg(short, long)]
        port: String,
        /// Output directory (created if missing)
        #[arg(short, long, default_value = "p64-dump")]
        out: PathBuf,
        /// Print every command/response on stderr
        #[arg(short, long)]
        verbose: bool,
    },
    /// Connect, print the handshake reply, and disconnect (quick liveness check).
    Info {
        #[arg(short, long)]
        port: String,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Decode a codeplug dump directory into an editable TOML config.
    Decode {
        /// Dump directory produced by `read`
        dump: PathBuf,
        /// Output TOML file
        #[arg(short, long, default_value = "radio.toml")]
        out: PathBuf,
        /// Country profile to record in the file (e.g. CH)
        #[arg(short, long, default_value = "CH")]
        country: String,
        /// Include expert fields (frequencies, timeslot, bandwidth) hidden by default
        #[arg(short = 'x', long)]
        expert: bool,
        /// Annotate each setting with an explanatory comment
        #[arg(long)]
        comments: bool,
    },
    /// Validate a TOML config against a country regulation profile.
    Check {
        config: PathBuf,
        /// Override the country in the file (e.g. CH)
        #[arg(short, long)]
        country: Option<String>,
    },
    /// Self-test: decode a dump, re-apply it, and confirm the bytes are unchanged.
    Roundtrip { dump: PathBuf },
    /// Write a codeplug to the radio. Base image = --from-dump, or read live.
    /// With no config, writes the base image back unchanged (identity test).
    Write {
        #[arg(short, long)]
        port: String,
        /// Optional TOML config to apply onto the base image before writing
        config: Option<PathBuf>,
        /// Base codeplug: a dump dir from `read`. If omitted, the radio is read first.
        #[arg(long)]
        from_dump: Option<PathBuf>,
        /// Country profile for the pre-write regulation check
        #[arg(short, long, default_value = "CH")]
        country: String,
        /// Write every region, even unchanged ones (needed for an identity write)
        #[arg(long)]
        all: bool,
        /// Required to actually write (safety gate)
        #[arg(long)]
        yes: bool,
        /// Skip the read-back verification after writing
        #[arg(long)]
        no_verify: bool,
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Info { port, verbose } => info(&port, verbose),
        Cmd::Read { port, out, verbose } => read(&port, out, verbose),
        Cmd::Decode {
            dump,
            out,
            country,
            expert,
            comments,
        } => decode(dump, out, &country, expert, comments),
        Cmd::Check { config, country } => check(config, country),
        Cmd::Roundtrip { dump } => roundtrip(dump),
        Cmd::Write {
            port,
            config,
            from_dump,
            country,
            all,
            yes,
            no_verify,
            verbose,
        } => write_radio(
            &port, config, from_dump, &country, all, yes, no_verify, verbose,
        ),
    }
}

fn payload_of(raw: &[u8]) -> &[u8] {
    let n = u16::from_le_bytes([raw[12], raw[13]]) as usize;
    &raw[14..14 + n]
}

#[allow(clippy::too_many_arguments)]
fn write_radio(
    port: &str,
    config: Option<PathBuf>,
    from_dump: Option<PathBuf>,
    country: &str,
    all: bool,
    yes: bool,
    no_verify: bool,
    verbose: bool,
) -> Result<()> {
    // 1. Obtain the base codeplug (a full 13-region image).
    let mut cp = match &from_dump {
        Some(dir) => {
            println!("Base image: {}", dir.display());
            codeplug::Codeplug::from_dump_dir(dir)?
        }
        None => {
            println!("No --from-dump given; reading the radio first as the base image...");
            let s = serial::Serial::open(port)?;
            let regions = proto::read_all(&s, verbose)?;
            let mut r = Vec::new();
            for rd in regions {
                r.push(codeplug::Region {
                    name: rd.name,
                    raw: rd.reply,
                });
            }
            codeplug::Codeplug { regions: r }
        }
    };

    // Snapshot the base region payloads so we can write only what changed.
    let base: std::collections::HashMap<String, Vec<u8>> = proto::WRITE_REGIONS
        .iter()
        .map(|(name, _)| {
            (
                name.to_string(),
                cp.region(name).unwrap().payload().to_vec(),
            )
        })
        .collect();

    // 2. Optionally apply an edited config, with a regulation check.
    if let Some(cfg_path) = &config {
        let text = std::fs::read_to_string(cfg_path)
            .with_context(|| format!("reading {}", cfg_path.display()))?;
        let cfg = config::from_toml(&text)?;
        if let Some(p) = regs::profile_for(country) {
            let findings = regs::check(&cfg, p);
            let (errors, warnings) = regs::print_findings(&findings);
            println!(
                "Regulation check ({}): {errors} error(s), {warnings} warning(s)",
                p.name
            );
            if errors > 0 {
                anyhow::bail!(
                    "refusing to write: config violates {} (fix the errors)",
                    p.name
                );
            }
        }
        config::apply(&mut cp, &cfg)?;
        println!(
            "Applied {} onto the base image ({} channels).",
            cfg_path.display(),
            cfg.channel.len()
        );
    } else if all {
        println!("No config given: IDENTITY write of all regions (base written back unchanged).");
    } else {
        anyhow::bail!(
            "nothing to write: give a config to apply, or use --all for a full/identity write"
        );
    }

    // 3. Decide which regions to write. Default: only regions whose bytes
    //    changed vs the base. `--all` forces every region.
    let mut frames = Vec::new();
    let mut skipped = Vec::new();
    for (name, id) in proto::WRITE_REGIONS {
        let payload = cp.region(name)?.payload().to_vec();
        let changed = base.get(*name).map(|b| b != &payload).unwrap_or(true);
        if all || changed {
            frames.push((name.to_string(), proto::build_write_frame(*id, &payload)));
        } else {
            skipped.push(*name);
        }
    }
    if frames.is_empty() {
        println!("No regions differ from the base - nothing to write.");
        return Ok(());
    }
    let written: Vec<&str> = frames.iter().map(|(n, _)| n.as_str()).collect();
    println!("Regions to write: {}", written.join(", "));
    if !skipped.is_empty() {
        println!("Unchanged (skipped): {}", skipped.join(", "));
    }

    // 4. Safety gate.
    if !yes {
        println!(
            "\nAbout to write {} region(s) to the radio on {port}.",
            frames.len()
        );
        println!("This modifies the radio. Re-run with --yes to proceed.");
        return Ok(());
    }

    // 5. Write.
    let s = serial::Serial::open(port)?;
    proto::write_all(&s, &frames, verbose)?;
    println!("Write complete.");

    // 6. Verify by reading back.
    if no_verify {
        return Ok(());
    }
    println!("\nVerifying (reading back)...");
    let s2 = serial::Serial::open(port)?;
    let readback = proto::read_all(&s2, verbose)?;
    let mut mismatch = 0usize;
    for rd in &readback {
        let want = cp.region(&rd.name)?.payload();
        let got = payload_of(&rd.reply);
        let n = want.len().min(got.len());
        let diffs = (0..n).filter(|&i| want[i] != got[i]).count() + want.len().abs_diff(got.len());
        if diffs == 0 {
            println!("  {}: verified ({} bytes)", rd.name, want.len());
        } else {
            println!("  {}: {diffs} byte(s) differ vs intended!", rd.name);
            mismatch += diffs;
        }
    }
    if mismatch == 0 {
        println!("\nVerification OK: the radio now matches the intended image.");
    } else {
        anyhow::bail!("verification found {mismatch} differing byte(s) - do NOT trust this write");
    }
    Ok(())
}

fn roundtrip(dump: PathBuf) -> Result<()> {
    let cp = codeplug::Codeplug::from_dump_dir(&dump)?;
    let cfg = config::decode(&cp, "CH", true)?;
    // apply onto a fresh copy and diff the managed regions
    let mut cp2 = codeplug::Codeplug::from_dump_dir(&dump)?;
    config::apply(&mut cp2, &cfg)?;
    let mut total = 0usize;
    for name in codeplug::REGION_ORDER {
        let a = cp.region(name)?.payload();
        let b = cp2.region(name)?.payload();
        let diffs: Vec<usize> = (0..a.len().min(b.len()))
            .filter(|&i| a[i] != b[i])
            .collect();
        if diffs.is_empty() {
            println!("{name}: identical ({} bytes)", a.len());
        } else {
            println!(
                "{name}: {} byte(s) differ after decode->apply:",
                diffs.len()
            );
            for &i in diffs.iter().take(20) {
                println!("    payload[{i}]: {:#04x} -> {:#04x}", a[i], b[i]);
            }
        }
        total += diffs.len();
    }
    if total == 0 {
        println!("\nRoundtrip OK: decode->apply is byte-faithful. Write path is safe to build on.");
    } else {
        println!("\nRoundtrip has {total} diffs - investigate before writing to a radio.");
        std::process::exit(1);
    }
    Ok(())
}

fn decode(dump: PathBuf, out: PathBuf, country: &str, expert: bool, comments: bool) -> Result<()> {
    let cp = codeplug::Codeplug::from_dump_dir(&dump)?;
    let label = identity::r01_model_label(cp.region("r01")?.payload());
    if let Some(note) = identity::unknown_model_note(&label) {
        eprintln!("NOTE: {note}");
    }
    let cfg = config::decode(&cp, country, expert)?;
    let mut toml = config::to_toml(&cfg)?;
    if comments {
        toml = config::annotate(&toml);
    }
    std::fs::write(&out, &toml).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "Decoded {} channels + general settings -> {}{}",
        cfg.channel.len(),
        out.display(),
        if expert { " (expert)" } else { "" }
    );
    // opportunistic regulation check
    if let Some(p) = regs::profile_for(country) {
        let findings = regs::check(&cfg, p);
        if !findings.is_empty() {
            println!("\nRegulation check ({}):", p.name);
            let (e, w) = regs::print_findings(&findings);
            println!("  {e} error(s), {w} warning(s)");
        }
    }
    Ok(())
}

fn check(cfg_path: PathBuf, country: Option<String>) -> Result<()> {
    let text = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("reading {}", cfg_path.display()))?;
    let cfg = config::from_toml(&text)?;
    let country = country.unwrap_or_else(|| cfg.radio.country.clone());
    let profile = regs::profile_for(&country).ok_or_else(|| {
        anyhow::anyhow!("no regulation profile for country {:?} (try CH)", country)
    })?;
    println!(
        "Checking {} against {} ...",
        cfg_path.display(),
        profile.name
    );
    let findings = regs::check(&cfg, profile);
    let (errors, warnings) = regs::print_findings(&findings);
    println!("{errors} error(s), {warnings} warning(s)");
    if errors > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn info(port: &str, verbose: bool) -> Result<()> {
    let s = serial::Serial::open(port)?;
    let (mcu, r01) = proto::probe_identity(&s, verbose)?;
    let id = identity::from_probe(mcu, &r01);
    println!("MCU name : {}", id.mcu_name);
    println!("Firmware : {}", id.firmware);
    println!("Built    : {}", id.build_date);
    println!(
        "Model    : {}",
        id.model_label.as_deref().unwrap_or("(unknown)")
    );
    match identity::gate(&id) {
        identity::GateOutcome::Ok => println!("Gate     : OK (known P64 layout)"),
        identity::GateOutcome::UnknownVersion {
            model_label,
            firmware,
        } => {
            println!("Gate     : WARNING — {model_label}/{firmware} not in p64tool's validated set")
        }
        identity::GateOutcome::WrongModel { mcu_name } => {
            println!("Gate     : REFUSE writes — model {mcu_name:?} is not a P64")
        }
    }
    Ok(())
}

fn read(port: &str, out: PathBuf, verbose: bool) -> Result<()> {
    let s = serial::Serial::open(port)?;
    let regions = proto::read_all(&s, verbose)?;

    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;

    let mut combined = Vec::new();
    let mut manifest = String::new();
    manifest.push_str("# p64tool codeplug dump\n# region  selector  requested  received  header_ok  offset_in_combined\n");
    let mut all_ok = true;

    for r in &regions {
        let fname = out.join(format!("{}.bin", r.name));
        std::fs::write(&fname, &r.reply).with_context(|| format!("writing {}", fname.display()))?;
        manifest.push_str(&format!(
            "{:<6}  {:<8}  {:>9}  {:>8}  {:<9}  {}\n",
            r.name,
            proto::hex(&r.selector),
            r.requested,
            r.reply.len(),
            if r.prefix_ok { "yes" } else { "NO" },
            combined.len(),
        ));
        combined.extend_from_slice(&r.reply);
        if !r.prefix_ok {
            all_ok = false;
        }
    }

    let combined_path = out.join("codeplug_raw.bin");
    std::fs::write(&combined_path, &combined)?;
    let manifest_path = out.join("manifest.txt");
    std::fs::write(&manifest_path, &manifest)?;

    println!();
    println!(
        "Wrote {} region files + {} ({} bytes) to {}/",
        regions.len(),
        combined_path.display(),
        combined.len(),
        out.display()
    );
    println!("Manifest: {}", manifest_path.display());
    if all_ok {
        println!("All region headers matched the expected protocol signatures. [OK]");
    } else {
        println!("WARNING: some regions had unexpected headers - see manifest.txt.");
    }
    Ok(())
}
