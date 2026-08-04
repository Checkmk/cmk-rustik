use bytes::Bytes;
use reqwest::Client;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::time::{Duration, timeout};
use tracing::{debug, trace};

use crate::cli_args::CliArgs;
use crate::error::{Error, Result};
use crate::payload::Payload;
use crate::scraper::Scraper;

const AGENT_PATH: &str = "/usr/local/bin/check_mk_agent";
const AGENT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct LinuxAgentScraper {
    relay_client: Client,
    args: Arc<CliArgs>,
}

impl LinuxAgentScraper {
    pub(crate) fn new(args: Arc<CliArgs>) -> LinuxAgentScraper {
        LinuxAgentScraper {
            relay_client: Client::new(),
            args,
        }
    }
}

impl Scraper for LinuxAgentScraper {
    fn relay_client(&self) -> Client {
        self.relay_client.clone()
    }

    fn args(&self) -> Arc<CliArgs> {
        self.args.clone()
    }

    /// Run the local check_mk_agent script and capture its raw stdout.
    ///
    /// We do not parse the output or perform any calculations on it here,
    /// leaving these tasks for metrics-cache (and later, Checkmk itself) to do.
    async fn scrape(&self) -> Result<Payload> {
        let node_name = match std::env::var("NODE_NAME") {
            Ok(val) => val,
            Err(err) => {
                return Err(Error::EnvVar {
                    name: "NODE_NAME".to_string(),
                    source: err,
                });
            }
        };

        debug!("running check_mk_agent");
        let child = Command::new(AGENT_PATH)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let output = match timeout(AGENT_TIMEOUT, child.wait_with_output()).await {
            Ok(result) => result?,
            Err(_elapsed) => return Err(Error::AgentTimeout(AGENT_TIMEOUT)),
        };

        if !output.status.success() {
            return Err(Error::AgentExitStatus {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        if output.stdout.is_empty() {
            return Err(Error::AgentEmptyOutput);
        }

        trace!(bytes = output.stdout.len(), "check_mk_agent run complete");
        Ok(Payload::CheckmkLinuxAgent {
            node_name,
            body: Bytes::from(output.stdout),
        })
    }
}
