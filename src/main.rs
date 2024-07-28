#![warn(unused_extern_crates)]

use std::fs;
use std::process;
use std::io::{self,Write};
use std::u16;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport, Address};
use serde::{Serialize, Deserialize};
use reqwest;
use reqwest::header::{HeaderMap, HeaderValue,AUTHORIZATION,ACCEPT};
use tokio::signal;
use tokio::sync::watch;
use anyhow::{Context, Result};
use colored::*;
use chrono::Local;

#[derive(Serialize, Deserialize)]
struct Config {
    smtp_server: String,
    smtp_port: u16,
    smtp_user: String,
    smtp_pass: String,
    domain_name: String,
    email: String,
    log: bool,
    provider: String,
    api_key: String,
    frequency: u32, //in seconds
}

#[allow(unused)]

enum MessageType {
    Success,
    Info,
    Warning,
    Error,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_or_create_config().await.context("Failed to load or create config.")?;

    //Validate SMTP credentials
    if !validate_smtp(&config) {
        process::exit(1);
    }

    //I'm aware that this is an overkill. :) But I want to be able to watch multiple domains in the future versions, so.
    let (_shutdown_tx,mut shutdown_rx) = watch::channel(());
    
    tokio::spawn(async move {
        if let Err(e) = domain_check_loop(config,&mut shutdown_rx).await {
            print_message("Error in domain check loop",&e.to_string(),MessageType::Error);
        }
    });

    match signal::ctrl_c().await {
        Ok(()) => {
            print_message("Shutting down.","Bye-bye.",MessageType::Info);
        }
        Err(e) => {
            print_message("Unable to listen to shutdown signal", &e.to_string(),MessageType::Error);
        },        
    }
    
    Ok(())
}

fn print_message(short_message : &str,long_message : &str,message_type: MessageType) {
    print!("{} ",get_timestamp());

    match message_type {
        MessageType::Success => {
            print!("{}", short_message.green().bold());
        }
        MessageType::Info => {
            print!("{}", short_message.bold());
        }
        MessageType::Warning => {
            print!("{}", short_message.yellow().bold());
        }
        MessageType::Error => {
            print!("{}", short_message.red().bold());
        }
    }
    
    println!(" {}",long_message);
}

fn log_message(message: &str, config: &Config) {
    if config.log {
        let log_file = "domain_watcher.log";
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .unwrap();

        writeln!(file, "{} {}", get_timestamp(), message).unwrap();
    }
}

fn get_timestamp () -> String {
    Local::now().format("[ %d.%m.%Y %H:%M:%S ]").to_string()
}

async fn load_or_create_config() -> Result<Config> {
    print_message("Starting.","Attempting to load or create configuration file.",MessageType::Info);

    let config_path = "config.toml";

    //Check if the config file exists
    if !std::path::Path::new(config_path).exists() {
        let default_config = Config {
            smtp_server: String::new(),
            smtp_port: 465,
            smtp_user: String::new(),
            smtp_pass: String::new(),
            domain_name: String::new(),
            email: String::new(),
            log: false,
            provider: "whoisjson.com".to_string(), //multiple providers in future versions
            api_key: String::new(),
            frequency: 86400, //24 hours
        };
    
        fs::write(config_path,toml::to_string(&default_config)?).context("Failed to write default config")?;
        print_message(
            "Default config file created.",
            "Created default config file. You can edit it later with your settings.",
            MessageType::Success);

        let config = prompt_for_config()?;
        fs::write(config_path, toml::to_string(&config)?)?;
        
        print_message(
            "Configuration stored.",
            "",
            MessageType::Success);

        return Ok(config);
    }
    else {
        print_message(
            "Config file found.",
            "Loading.",
            MessageType::Info);

        let config: Config = toml::from_str(&fs::read_to_string(config_path)?)?;
        
        print_message(
            "Config file loaded.",
            "",
            MessageType::Success);

        return Ok(config);
    }
}

fn prompt_for_config() -> Result<Config> {
    let smtp_server = prompt("SMTP server: ");
    
    let smtp_port = loop {
        let input = prompt("SMTP port: ");

        match input.parse::<u16>() {
            Ok(value) => {
                if value > 0 {
                    break value;
                }
                else {
                    print_message("Not quite.","A port can't be zero, can it?",MessageType::Error);
                }
            }
            Err(_) => print_message("Not quite.","Invalid input. Please enter an integer.",MessageType::Error),
        }
    };

    let smtp_user = prompt("SMTP username: ");
    let smtp_pass = prompt("SMTP password: ");
    let domain_name = prompt("Domain name to watch: ");
    let email = prompt("Notification email: ");
    
    let log = loop {
        let input = prompt("Log results (true/false): ");

        match input.parse::<bool>() {
            Ok(value) => break value,
            Err(_) => print_message("Not quite.","Invalid input. Please enter 'true' or 'false'.",MessageType::Error),
        }
    };
    
    let provider = "whoisjson.com".to_string(); //Can be selected in future versions.
    let api_key = prompt("API key: ");

    let frequency = loop {
        let input = prompt("Check frequency (seconds): ");

        match input.parse::<u32>() {
            Ok(value) => break value,
            Err(_) => print_message("Not quite.","Invalid input. Please enter an integer.",MessageType::Error),
        }
    };

    return Ok(Config {
        smtp_server,
        smtp_port,
        smtp_user,
        smtp_pass,
        domain_name,
        email,
        log,
        provider,
        api_key,
        frequency,
    });
}

fn prompt(message: &str) -> String {
    print!("{}",message);
    io::stdout().flush().unwrap();

    let mut input = String::new();

    io::stdin().read_line(&mut input).unwrap();
    
    return input.trim().to_string();
}

fn validate_smtp(config: &Config) -> bool {
    print_message("Starting.","Attempting to validate SMTP credentials.",MessageType::Info);

    let from_address: Address = match config.smtp_user.parse() {
        Ok(addr) => addr,
        Err(e) => {
            print_message("Invalid mail",
                &format!("Invalid sender email address: {}. Please change the config file and run again.",e),
                MessageType::Error);
            return false;
        }
    };
    
    let to_address: Address = match config.email.parse() {
        Ok(addr) => addr,
        Err(e) => {
            print_message("Invalid mail",
                &format!("Invalid recipient email address: {}. Please change the config file and run again.",e),
                MessageType::Error);
            return false;
        }
    };

    let email = match Message::builder()
        .from(from_address.clone().into())
        .to(to_address.clone().into())
        .subject("SMTP validation Test")
        .body(format!("This is a test email to validate SMTP credentials.")) {
            Ok(msg) => msg,
            Err(e) => {
                print_message("Mail biulder error", &format!("Failed to build email message: {}",e),MessageType::Error);
                return false;
            }
        };

    let creds = Credentials::new(
        config.smtp_user.clone(),
        config.smtp_pass.clone()
    );

    let mailer = match SmtpTransport::relay(&config.smtp_server).map(|relay| relay.port(config.smtp_port))
    {
        Ok(relay) => relay.credentials(creds).build(),
        Err(e) => {
            print_message("SMTP relay error", &format!("Failed to create SMTP relay: {}",e),MessageType::Error);
            return false;
        }
    };

    match mailer.send(&email) {
        Ok(_) => {
            print_message("Passed.", "Check your inbox for the test email.", MessageType::Success);
            return true;
        },
        Err(e) => {
            print_message("Mail error",
                &format!("Failed to send email: {}. Please edit the configuration file and start again.",e),
                MessageType::Error);
            return false;
        }
    };
}

async fn check_domain_availability(domain: &str, api_key: &str) -> Result<bool> {
    
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT,HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION,HeaderValue::from_str(&format!("TOKEN={}",api_key))?);
    let params: [(&str, &str); 2]= [
        ("domain",domain),
        ("format","json"),    
    ];

    let client = reqwest::Client::new();
    let response = client.get("https://whoisjson.com/api/v1/whois")
                .headers(headers)
                .query(&params)
                .send()
                .await
                .context("Failed to send request.");

    let response_result = match response {
        Ok(ref response_result) => response_result,
        Err(e) => {
            return Err(e);
        }
    };

    let response_status = response_result.status();

    if response_status.is_client_error() || response_status.is_server_error() {
        let error_json = response?.json().await.unwrap_or_else(|_| {
            serde_json::json!({"message":"No error message provided by API."})
        });
        let error_message = error_json.get("message").and_then(|m| m.as_str()).unwrap_or("No error message provided by API.");
        print_message(&format!("Error {}: ",response_status), error_message,MessageType::Error);

        return Err(anyhow::anyhow!("Request failed with status {}: {}",response_status,error_message));
    }

    let json: serde_json::Value = response?.json().await.context("Failed to parse JSON.")?;

    return Ok(!json["registered"].as_bool().unwrap_or(false));
}

fn send_email(config: &Config) {
    let email = Message::builder()
        .from(config.smtp_user.parse().unwrap())
        .to(config.email.parse().unwrap())
        .subject(format!("Domain {} is available!", config.domain_name))
        .body(format!("Domain available! Go get it!"))
        .unwrap();

    let creds = lettre::transport::smtp::authentication::Credentials::new(
        config.smtp_user.clone(),
        config.smtp_pass.clone()
    );

    let mailer = SmtpTransport::relay(&config.smtp_server).map(|relay| relay.port(config.smtp_port))
        .unwrap()
        .credentials(creds)
        .build();

    match mailer.send(&email) {
        Ok(_) => {},
        Err(e) => {
            print_message("Error",&format!("Could not send email: {:?}",e),MessageType::Error);
        },        
    }
}

async fn domain_check_loop(config: Config,shutdown_rx: &mut watch::Receiver<()>) -> Result<()> {
    loop {
        match check_domain_availability(&config.domain_name,&config.api_key).await {
            Ok(available) => {
                if available {
                    send_email(&config);
        
                    let short_message=format!("Domain {} is available!",&config.domain_name);
        
                    print_message(&short_message,"Notification sent. Exiting.",MessageType::Success);
                    log_message(&(short_message + " Notification sent. Exiting."), &config);
                    process::exit(0);
                }
                else {
                    let short_message=format!("Domain {} is still not available. Keep lurking.",&config.domain_name);
        
                    print_message("Registered",&short_message,MessageType::Info);
                    log_message(&short_message, &config);
                }
            }
            Err(e) => {
                let error_message = format!("Quitting with error: {:?}",e);
                print_message("Error",&error_message,MessageType::Error);
                log_message(&error_message,&config);
                process::exit(1);
            }
        }
        
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }

            _ = tokio::time::sleep(std::time::Duration::from_secs(config.frequency.into())) => {}
        }
    }
        
    

    Ok(())
}