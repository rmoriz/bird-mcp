//! Running BIRD's command-line client over a local or ssh transport.

use std::io::ErrorKind;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::{Router, TransportKind};
use crate::error::{ToolError, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// What to show a caller when the command failed: stderr first, then stdout.
    pub fn detail(&self) -> String {
        let stderr = self.stderr.trim();
        let stdout = self.stdout.trim();
        match (stderr.is_empty(), stdout.is_empty()) {
            (false, _) => stderr.to_string(),
            (true, false) => stdout.to_string(),
            (true, true) => "no output".to_string(),
        }
    }
}

impl Router {
    /// Runs one birdc command line. The line is passed as a single argument, so
    /// birdc receives it verbatim (it joins argv with spaces).
    pub async fn run(&self, line: &str) -> ToolResult<Output> {
        let mut command = match self.transport {
            TransportKind::Local => {
                let mut command = Command::new(&self.birdc);
                if let Some(socket) = &self.socket {
                    command.arg("-s").arg(socket);
                }
                command.arg(line);
                command
            }
            TransportKind::Ssh => {
                let mut command = Command::new(&self.ssh_bin);
                command
                    .arg("-o")
                    .arg("BatchMode=yes")
                    .arg("-o")
                    .arg("ConnectTimeout=10");
                if let Some(port) = self.port {
                    command.arg("-p").arg(port.to_string());
                }
                command.arg(self.ssh_destination());
                command.arg("--");
                command.arg(self.remote_command(line));
                command
            }
        };

        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        tracing::debug!(router = %self.name, command = %line, "running birdc");
        let output = self.run_command(command, line, None).await?;

        Ok(Output {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code(),
        })
    }

    /// Runs birdc and turns a non-zero exit code into an error.
    pub async fn run_checked(&self, line: &str) -> ToolResult<Output> {
        let output = self.run(line).await?;
        if output.success() {
            Ok(output)
        } else {
            Err(ToolError::CommandFailed {
                router: self.name.clone(),
                command: line.to_string(),
                status: match output.code {
                    Some(code) => format!(" (exit {code})"),
                    None => " (killed)".to_string(),
                },
                detail: output.detail(),
            })
        }
    }

    /// Reads a file from the machine that runs BIRD. Used for config inspection.
    pub async fn read_file(&self, path: &str) -> ToolResult<String> {
        validate_path(path)?;

        match self.transport {
            TransportKind::Local => tokio::fs::read_to_string(path)
                .await
                .map_err(|err| ToolError::Io(format!("cannot read {path}: {err}"))),
            TransportKind::Ssh => {
                let line = format!("cat {}", shell_quote(path));
                let output = self.run_shell(&line).await?;
                if !output.success() {
                    return Err(ToolError::CommandFailed {
                        router: self.name.clone(),
                        command: line,
                        status: match output.code {
                            Some(code) => format!(" (exit {code})"),
                            None => " (killed)".to_string(),
                        },
                        detail: output.detail(),
                    });
                }
                Ok(output.stdout)
            }
        }
    }

    /// Writes a file on the machine that runs BIRD. The write goes to a temp
    /// file first, so a failed transfer never leaves a half-written config.
    pub async fn write_file(&self, path: &str, content: &str) -> ToolResult<()> {
        validate_path(path)?;
        let temp = format!("{path}.bird-mcp.tmp");

        match self.transport {
            TransportKind::Local => {
                tokio::fs::write(&temp, content)
                    .await
                    .map_err(|err| ToolError::Io(format!("cannot write {temp}: {err}")))?;
                tokio::fs::rename(&temp, path)
                    .await
                    .map_err(|err| ToolError::Io(format!("cannot replace {path}: {err}")))?;
            }
            TransportKind::Ssh => {
                let line = format!(
                    "cat > {} && mv -f {} {}",
                    shell_quote(&temp),
                    shell_quote(&temp),
                    shell_quote(path)
                );
                let output = self
                    .run_shell_with_stdin(&line, Some(content.as_bytes()))
                    .await?;
                if !output.success() {
                    return Err(ToolError::CommandFailed {
                        router: self.name.clone(),
                        command: line,
                        status: match output.code {
                            Some(code) => format!(" (exit {code})"),
                            None => " (killed)".to_string(),
                        },
                        detail: output.detail(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Reads a file when it exists; `None` means it is not there yet.
    pub async fn read_file_optional(&self, path: &str) -> ToolResult<Option<String>> {
        validate_path(path)?;

        match self.transport {
            TransportKind::Local => match tokio::fs::read_to_string(path).await {
                Ok(content) => Ok(Some(content)),
                Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
                Err(err) => Err(ToolError::Io(format!("cannot read {path}: {err}"))),
            },
            TransportKind::Ssh => {
                let test = format!("test -e {}", shell_quote(path));
                let output = self.run_shell(&test).await?;
                if !output.success() {
                    return Ok(None);
                }
                self.read_file(path).await.map(Some)
            }
        }
    }

    /// Removes a file if it exists.
    pub async fn remove_file(&self, path: &str) -> ToolResult<()> {
        validate_path(path)?;

        match self.transport {
            TransportKind::Local => match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
                Err(err) => Err(ToolError::Io(format!("cannot remove {path}: {err}"))),
            },
            TransportKind::Ssh => {
                let line = format!("rm -f {}", shell_quote(path));
                let output = self.run_shell(&line).await?;
                if !output.success() {
                    return Err(ToolError::CommandFailed {
                        router: self.name.clone(),
                        command: line,
                        status: match output.code {
                            Some(code) => format!(" (exit {code})"),
                            None => " (killed)".to_string(),
                        },
                        detail: output.detail(),
                    });
                }
                Ok(())
            }
        }
    }

    /// Runs a plain shell command through ssh. Only used with paths and commands
    /// this crate builds; never with caller-supplied text.
    async fn run_shell(&self, line: &str) -> ToolResult<Output> {
        self.run_shell_with_stdin(line, None).await
    }

    async fn run_shell_with_stdin(&self, line: &str, input: Option<&[u8]>) -> ToolResult<Output> {
        let mut command = Command::new(&self.ssh_bin);
        command
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=10");
        if let Some(port) = self.port {
            command.arg("-p").arg(port.to_string());
        }
        command.arg(self.ssh_destination()).arg("--").arg(line);
        command
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = self.run_command(command, line, input).await?;
        Ok(Output {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code(),
        })
    }

    /// Runs one command with the router timeout and a short retry for
    /// `ETXTBSY`. Linux returns that when a just-written script is still open
    /// for writing in a forked child, which happens with busy test suites (and
    /// when an operator replaces birdc under a running server).
    async fn run_command(
        &self,
        mut command: Command,
        label: &str,
        input: Option<&[u8]>,
    ) -> ToolResult<std::process::Output> {
        let deadline = tokio::time::Instant::now() + self.timeout;
        let mut attempts = 0;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(self.timeout_error(label));
            }

            let attempt = async {
                let mut child = command.spawn()?;
                if let Some(input) = input
                    && let Some(mut stdin) = child.stdin.take()
                {
                    stdin.write_all(input).await?;
                    stdin.shutdown().await?;
                }
                child.wait_with_output().await
            };

            match tokio::time::timeout(remaining, attempt).await {
                Err(_) => return Err(self.timeout_error(label)),
                Ok(Ok(output)) => return Ok(output),
                Ok(Err(source))
                    if source.kind() == ErrorKind::ExecutableFileBusy && attempts < 10 =>
                {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Ok(Err(source)) => {
                    return Err(ToolError::Spawn {
                        command: self.program_name().to_string(),
                        source,
                    });
                }
            }
        }
    }

    fn timeout_error(&self, label: &str) -> ToolError {
        ToolError::Timeout {
            router: self.name.clone(),
            command: label.to_string(),
            secs: self.timeout.as_secs(),
        }
    }

    fn program_name(&self) -> &str {
        match self.transport {
            TransportKind::Local => &self.birdc,
            TransportKind::Ssh => &self.ssh_bin,
        }
    }

    fn ssh_destination(&self) -> String {
        match (&self.user, &self.host) {
            (Some(user), Some(host)) => format!("{user}@{host}"),
            (None, Some(host)) => host.clone(),
            _ => String::new(),
        }
    }

    /// The command ssh hands to the remote shell. Every component is quoted so
    /// filters and patterns survive the remote shell untouched.
    fn remote_command(&self, line: &str) -> String {
        let mut parts = vec![shell_quote(&self.birdc)];
        if let Some(socket) = &self.socket {
            parts.push("-s".to_string());
            parts.push(shell_quote(socket));
        }
        parts.push(shell_quote(line));
        parts.join(" ")
    }
}

/// Rejects paths that could escape a single shell word. Paths come from
/// configuration, but a typo should fail loudly rather than run something else.
fn validate_path(path: &str) -> ToolResult<()> {
    if !path.starts_with('/') {
        return Err(ToolError::BadArgs(format!(
            "config file path must be absolute: {path}"
        )));
    }
    if path.contains('\n') || path.contains('\r') || path.contains('\0') {
        return Err(ToolError::BadArgs("path contains a line break".to_string()));
    }
    Ok(())
}

/// Quotes one word for a POSIX shell.
pub fn shell_quote(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('\'');
    for character in word.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}
