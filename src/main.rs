//! simyo-esim — fetch Simyo eSIM info directly from the official API.
//!
//! Credentials only travel between this machine and `appapi.simyo.nl`.
//! The password is never written to disk or logs.

mod api;
mod flow;
mod lpa;

use std::io::{self, Write};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use tracing::{info, warn};

#[derive(Parser)]
#[command(
    name = "simyo-esim",
    version,
    about = "Fetch Simyo eSIM info directly from the official API (no third-party server)"
)]
struct Cli {
    /// Verbose logging (RUST_LOG also works)
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Full flow: login -> (MFA) -> (device change) -> fetch eSIM -> LPA
    Get(GetArgs),
    /// Login only; prints the session token on stdout
    Login(LoginArgs),
    /// Query /settings/simcard order status
    Simcard(TokenArg),
    /// Build an LPA string / QR from an activation code
    Lpa(LpaArgs),
    /// Confirm eSIM installation
    Confirm(TokenArg),
}

#[derive(Args)]
struct GetArgs {
    /// NL phone number (06xxxxxxxx)
    #[arg(long)]
    phone: Option<String>,
    /// Password (prefer env SIMYO_PASSWORD or interactive prompt)
    #[arg(long)]
    password: Option<String>,
    /// Existing session token (skips login)
    #[arg(long)]
    token: Option<String>,
    /// MFA OTP code (prompts if needed and not given)
    #[arg(long)]
    otp: Option<String>,
    /// Email validation code for device change
    #[arg(long)]
    code: Option<String>,
    /// Run the device-change flow before fetching
    #[arg(long)]
    new_device: bool,
    /// Render the LPA QR code in the terminal
    #[arg(long)]
    qr: bool,
    /// Confirm eSIM installation after fetching
    #[arg(long)]
    confirm: bool,
}

#[derive(Args)]
struct LoginArgs {
    #[arg(long)]
    phone: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    otp: Option<String>,
}

#[derive(Args)]
struct TokenArg {
    #[arg(long)]
    token: Option<String>,
}

#[derive(Args)]
struct LpaArgs {
    /// Activation code (prompts if not given)
    #[arg(long)]
    code: Option<String>,
    #[arg(long)]
    qr: bool,
}

fn main() {
    let cli = Cli::parse();
    let default_filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter)),
        )
        .with_target(false)
        .init();

    if let Err(e) = run(cli) {
        tracing::error!("{e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Get(a) => cmd_get(&a),
        Commands::Login(a) => cmd_login(&a),
        Commands::Simcard(a) => cmd_simcard(&a),
        Commands::Lpa(a) => cmd_lpa(&a),
        Commands::Confirm(a) => cmd_confirm(&a),
    }
}

// ---- credential / secret helpers (never logged, never persisted) ----

fn resolve_password(arg: Option<&str>) -> Result<String> {
    if let Some(p) = arg {
        return Ok(p.to_string());
    }
    if let Ok(p) = std::env::var("SIMYO_PASSWORD") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    rpassword::prompt_password("Simyo password: ").context("failed to read password")
}

fn prompt_phone(arg: Option<&str>) -> Result<String> {
    if let Some(p) = arg {
        return Ok(p.to_string());
    }
    if let Ok(p) = std::env::var("SIMYO_PHONE") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    print!("Phone number (06xxxxxxxx): ");
    io::stdout().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn prompt_secret(label: &str) -> Result<String> {
    rpassword::prompt_password(format!("{label}: ")).context("failed to read secret")
}

fn session_from(arg: Option<&str>) -> Option<String> {
    arg.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("SIMYO_SESSION_TOKEN")
                .ok()
                .filter(|s| !s.is_empty())
        })
}

fn redact(token: &str) -> String {
    if token.len() > 8 {
        format!("{}...{}", &token[..4], &token[token.len() - 4..])
    } else {
        "<short>".to_string()
    }
}

// ---- flows ----

/// Login (with optional MFA round-trip). Returns the final session token.
fn login_flow(
    client: &api::ApiClient,
    phone: Option<&str>,
    password: Option<&str>,
    otp: Option<&str>,
) -> Result<String> {
    let phone = prompt_phone(phone)?;
    if !flow::validate_phone(&phone) {
        bail!("invalid NL phone number, expected 06xxxxxxxx");
    }
    let password = resolve_password(password)?;

    let body = client.login(&phone, &password)?;
    let temp = flow::extract_session_token(&body).context("no sessionToken in login response")?;
    let mfa_status = flow::extract_mfa_status(&body);

    if flow::is_mfa_pending(mfa_status.as_deref()) {
        let status = mfa_status.clone().unwrap_or_else(|| "PENDING".to_string());
        let method = flow::extract_mfa_method(&body);
        info!(
            "login requires MFA ({status}{}), verifying OTP",
            method
                .as_deref()
                .map(|m| format!(" via {m}"))
                .unwrap_or_default()
        );
        let otp = match otp {
            Some(o) => o.to_string(),
            None => prompt_secret("OTP code")?,
        };
        if !flow::validate_code(&otp) {
            bail!("OTP must be 6 digits");
        }
        let vbody = client.verify_otp(&temp, &otp)?;
        let token =
            flow::extract_formal_token(&vbody).context("no formal token in verifyOTP response")?;
        info!("MFA verified, session {}", redact(&token));
        return Ok(token);
    }

    info!("login ok, session {}", redact(&temp));
    Ok(temp)
}

fn device_change_flow(client: &api::ApiClient, session: &str, code: Option<&str>) -> Result<()> {
    let status_body = client.get_simcard(session)?;
    let status = status_body
        .get("result")
        .and_then(|r| r.get("eSimStatus"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    match status {
        flow::ESIM_READY_FOR_DOWNLOAD => {
            info!("eSIM already ready for download, skipping order");
            return Ok(());
        }
        flow::ESIM_WAITING_FOR_VALIDATION_CODE => {
            info!("order already pending, validation code expected");
        }
        flow::ESIM_START_REQUEST => {
            info!("eSIM order start requested, proceeding to order");
        }
        other => {
            if !other.is_empty() {
                warn!("unexpected eSimStatus: {other}, requesting anyway");
            }
        }
    }
    client.apply_new_esim(session)?;
    info!("device change requested — check your email for the 6-digit code");

    let code = match code {
        Some(c) => c.to_string(),
        None => prompt_secret("Email validation code")?,
    };
    if !flow::validate_code(&code) {
        bail!("validation code must be 6 digits");
    }
    client.verify_code(session, &code)?;
    info!("device change verified, profile ready for download");
    Ok(())
}

// ---- commands ----

fn cmd_login(a: &LoginArgs) -> Result<()> {
    let client = api::ApiClient::new()?;
    let token = login_flow(
        &client,
        a.phone.as_deref(),
        a.password.as_deref(),
        a.otp.as_deref(),
    )?;
    println!("{token}");
    Ok(())
}

fn cmd_get(a: &GetArgs) -> Result<()> {
    let client = api::ApiClient::new()?;
    let session = match session_from(a.token.as_deref()) {
        Some(t) => t,
        None => login_flow(
            &client,
            a.phone.as_deref(),
            a.password.as_deref(),
            a.otp.as_deref(),
        )?,
    };

    if a.new_device {
        device_change_flow(&client, &session, a.code.as_deref())?;
    }

    let body = client.get_esim(&session)?;
    let info = flow::extract_esim_info(&body)
        .context("no activationCode in esim/get-by-customer response")?;

    println!("activationCode : {}", info.activation_code);
    if let Some(s) = &info.status {
        println!("status         : {s}");
    }
    if let Some(p) = &info.phone_number {
        println!("phoneNumber    : {p}");
    }
    if let Some(i) = &info.iccid {
        println!("iccid          : {i}");
    }

    let lpa = lpa::build_lpa(&info.activation_code);
    println!("LPA            : {lpa}");
    if a.qr {
        if let Some(qr) = lpa::render_qr(&lpa) {
            println!("{qr}");
        } else {
            warn!("failed to render QR code");
        }
    }

    if a.confirm {
        let r = client.confirm_install(&session)?;
        println!("confirm        : {r}");
    }
    Ok(())
}

fn cmd_simcard(a: &TokenArg) -> Result<()> {
    let client = api::ApiClient::new()?;
    let session = session_from(a.token.as_deref())
        .context("session token required (--token or SIMYO_SESSION_TOKEN)")?;
    let body = client.get_simcard(&session)?;
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}

fn cmd_lpa(a: &LpaArgs) -> Result<()> {
    let code = match &a.code {
        Some(c) => c.clone(),
        None => {
            print!("Activation code: ");
            io::stdout().flush().ok();
            let mut s = String::new();
            io::stdin().read_line(&mut s)?;
            s.trim().to_string()
        }
    };
    let lpa = lpa::build_lpa(&code);
    println!("{lpa}");
    if a.qr {
        if let Some(qr) = lpa::render_qr(&lpa) {
            println!("{qr}");
        } else {
            warn!("failed to render QR code");
        }
    }
    Ok(())
}

fn cmd_confirm(a: &TokenArg) -> Result<()> {
    let client = api::ApiClient::new()?;
    let session = session_from(a.token.as_deref())
        .context("session token required (--token or SIMYO_SESSION_TOKEN)")?;
    let body = client.confirm_install(&session)?;
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}
