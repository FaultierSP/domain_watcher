#![warn(unused_extern_crates)]
use anyhow::{anyhow, Result};
use print_and_log::*;
use serde::{Serialize, Deserialize};
use std::{fs,thread,time,process};
use toml;
use std::io::{self, Write};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport, Address};
use reqwest;
use reqwest::header::{HeaderMap, HeaderValue,AUTHORIZATION,ACCEPT};

#[derive(Serialize, Deserialize)]
struct Config {
    smtp_server: String,
    smtp_port: u16,
    smtp_user: String,
    smtp_pass: String,
    domain_names: String,
    from: String,
    email: String,
    log: bool,
    provider: String,
    api_key: String,
    frequency: u32, //in seconds
}

static CONFIG_PATH: &str = "config.toml";
static API_URL: &str = "https://whoisjson.com/api/v1/whois";

fn main () {
    let mut pal = PrintAndLog::new();
    let _ = pal.set_log_file_name("domain_watcher.log");
    pal.set_log_to_file(true);

    let config = load_or_create_config(&pal)
        .map_err(|e| pal.print("Error",e.to_string(),&PALMessageType::Error)).unwrap();

    pal.print("Starting.","Attempting to validate SMTP credentials.",&PALMessageType::Info);


    match validate_smtp(&config) {
        Ok(_) => {
            pal.print_and_log("Test passed.","Check your inbox for the test mail.",&PALMessageType::Success);
        },
        Err(e) => {
            pal.print_and_log(
                "Configuration error.",
                e.to_string(),
                &PALMessageType::Error);
            
            pal.print("",format!("Please edit the {} and start again.",CONFIG_PATH),&PALMessageType::Info);

            process::exit(1);
        }
    };

    let mut domain_names: Vec<&str> = config.domain_names.split(",").collect();

    loop {
        let mut indices_to_remove: Vec<usize> = Vec::new();

        for (i,domain) in domain_names.iter().enumerate() {
            println!("{}",domain);
            
            match check_domain_availability(domain,&config.api_key, &pal) {
                Ok(available) => {
                    if available {
                        match send_success_email(&config, domain) {
                            Ok(_) => {
                                indices_to_remove.push(i);
                            },
                            Err(e) => {
                                pal.print_and_log("Error",&format!("Could not send email: {:?}",e),&PALMessageType::Error);
                            }
                        }
                    }
                    else {
                        pal.print_and_log(
                            "Registered",
                            format!("Domain {} is still not available. Keep lurking.",domain),
                            &PALMessageType::Info
                        );
                    }
                },
                Err(e) => {
                    pal.print_and_log("Error",e.to_string(),&PALMessageType::Error);
                },
            };
        }

        for &index in indices_to_remove.iter() {
            domain_names.remove(index);
        }

        if domain_names.is_empty() {
            pal.print_and_log(
                "Completed",
                "All domains have been processed.",
                &PALMessageType::Info,
            );
            process::exit(0);
        }

        let sleeping_time = time::Duration::from_secs(config.frequency.into());
        thread::sleep(sleeping_time);

    }
}

fn load_or_create_config(pal: &PrintAndLog) -> Result<Config> {
    pal.print("Starting.","Attempting to load or create configuration file.",&PALMessageType::Info);

    if !std::path::Path::new(CONFIG_PATH).exists() {
        let default_config = Config {
            smtp_server: String::new(),
            smtp_port: 465,
            smtp_user: String::new(),
            smtp_pass: String::new(),
            domain_names: String::new(),
            from: String::new(),
            email: String::new(),
            log: false,
            provider: "whoisjson.com".to_string(), //multiple providers in future versions
            api_key: String::new(),
            frequency: 86400, //24 hours
        };

        let toml_string= toml::to_string(&default_config)
            .map_err(|e| anyhow!("Failed to serialize config to TOML: {}",e))?;

        fs::write(CONFIG_PATH,toml_string)
            .map_err(|e| anyhow!("Failed to write to the config file {}: {}",CONFIG_PATH,e))?;

        pal.print("Default file created.","Created default config file. You can edit it later with your settings.",&PALMessageType::Success);
    
        let config = prompt_for_config(&pal)?;

        let toml_string= toml::to_string(&config)
            .map_err(|e| anyhow!("Failed to serialize config to TOML: {}",e))?;

        fs::write(CONFIG_PATH,&toml_string)
            .map_err(|e| anyhow!("Failed to write to the config file {}: {}",CONFIG_PATH,e))?;

        pal.print("Config written.",format!("File {} saved. You can modify it later.",CONFIG_PATH),&PALMessageType::Success);

        return Ok(config);
    }
    else {
        pal.print("Config file found.","Loading.",&PALMessageType::Success);

        let string_from_the_file=fs::read_to_string(CONFIG_PATH)
            .map_err(|e| anyhow!("Failed to read from the config file {}: {}",CONFIG_PATH,e))?;

        let config = toml::from_str(&string_from_the_file)
            .map_err(|e| anyhow!("Couldn't convert file content to config: {}",e))?;

        return Ok(config);

    }
}

fn prompt_for_config(pal: &PrintAndLog) -> Result<Config> {
    let smtp_server = prompt("SMTP server: ");
    
    let smtp_port = loop {
        let input = prompt("SMTP port: ");

        match input.parse::<u16>() {
            Ok(value) => {
                if value > 0 {
                    break value;
                }
                else {
                    pal.print("Not quite.","A port can't be zero, can it?",&PALMessageType::Error);
                }
            }
            Err(_) => pal.print("Not quite.","Invalid input. Please enter an integer.",&PALMessageType::Error),
        }
    };

    let smtp_user = prompt("SMTP username: ");
    let smtp_pass = prompt("SMTP password: ");
    let domain_names = prompt("Domain name to watch: ");
    let from = prompt("Sender email: ");
    let email = prompt("Notification email: ");
    
    let log = loop {
        let input = prompt("Log results (true/false): ");

        match input.parse::<bool>() {
            Ok(value) => break value,
            Err(_) => pal.print("Not quite.","Invalid input. Please enter 'true' or 'false'.",&PALMessageType::Error),
        }
    };
    
    let provider = "whoisjson.com".to_string(); //Can be selected in future versions.
    let api_key = prompt("API key: ");

    let frequency = loop {
        let input = prompt("Check frequency (seconds): ");

        match input.parse::<u32>() {
            Ok(value) => break value,
            Err(_) => pal.print("Not quite.","Invalid input. Please enter an integer.",&PALMessageType::Error),
        }
    };

    return Ok(Config {
        smtp_server,
        smtp_port,
        smtp_user,
        smtp_pass,
        domain_names,
        from,
        email,
        log,
        provider,
        api_key,
        frequency,
    });
}

fn prompt(message: &str) -> String {
    print!("{} > ",message);
    io::stdout().flush().unwrap();

    let mut input = String::new();

    io::stdin().read_line(&mut input).unwrap();
    
    return input.trim().to_string();
}

fn validate_smtp(config: &Config) -> Result<bool> {

    let smtp_user: String = match config.smtp_user.is_empty() {
        false => config.smtp_user.clone(),
        true =>  return Err(anyhow!("Invalid SMTP username: username is empty."))
    };

    let smtp_password: String = match config.smtp_pass.is_empty() {
        false => config.smtp_pass.clone(),
        true =>  return Err(anyhow!("Invalid SMTP password: password is empty."))
    };

    let from_address: Address = match config.from.parse() {
        Ok(addr) => addr,
        Err(e) => return Err(anyhow!("Invalid sender email address. {}.",e))
    };

    let to_address: Address = match config.email.parse() {
        Ok(addr) => addr,
        Err(e) => return Err(anyhow!("Invalid recipient email address. {}.",e))
    };

    let email = match Message::builder()
        .from(from_address.into())
        .to(to_address.into())
        .subject("SMTP validation test from domain_watcher application")
        .body(format!("This is a test email to validate SMTP credentials."))
        {
            Ok(msg) => msg,
            Err(e) => return Err(anyhow!("Failed to build email message: {}",e))
        };
    
    let credentials = Credentials::new(smtp_user,smtp_password);

    let mailer = match SmtpTransport::relay(&config.smtp_server).map(|relay| relay.port(config.smtp_port)) {
        Ok(relay) => relay.credentials(credentials).build(),
        Err(e) => return Err(anyhow!("Failed to create SMTP relay: {}",e))
    };

    match mailer.send(&email) {
        Ok(_) => {},
        Err(e) => return Err(anyhow!("Failed to send email: {}",e))
    }

    return Ok(true);
}

fn check_domain_availability(domain: &str, api_key: &str, pal: &PrintAndLog) -> Result<bool> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT,HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION,HeaderValue::from_str(&format!("TOKEN={}",api_key))?);
    let params: [(&str, &str); 2]= [
        ("domain",domain),
        ("format","json"),    
    ];

    let client = reqwest::blocking::Client::new();
    let response = client.get(API_URL)
                                        .headers(headers)
                                        .query(&params)
                                        .send();
    
    let response_result = match response {
        Ok(ref response_result) => response_result,
        Err(e) => return Err(e.into()),
    };

    let response_status = response_result.status();

    if response_status.is_client_error() || response_status.is_server_error() {
        let error_json = response?.json().unwrap_or_else(|_| {
            serde_json::json!({"message":"No error message provided by API."})
        });
        let error_message = error_json.get("message").and_then(|m| m.as_str()).unwrap_or("No error message provided by API.");
        pal.print(&format!("Error {}: ",response_status), error_message,&PALMessageType::Error);

        return Err(anyhow::anyhow!("Request failed with status {}: {}",response_status,error_message));
    }

    let json: serde_json::Value = response?.json()?;

    return Ok(!json["registered"].as_bool().unwrap_or(false));
}

fn send_success_email(config: &Config, domain_name: &str) -> Result<(),String> {

    let email = Message::builder()
        .from(config.smtp_user.parse().unwrap())
        .to(config.email.parse().unwrap())
        .subject(format!("Domain {} is available!", domain_name))
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
        Ok(_) => return Ok(()),
        Err(e) => return Err(e.to_string()),        
    }
}