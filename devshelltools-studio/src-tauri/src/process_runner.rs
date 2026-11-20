use crate::error::{DstError, DstResult};
use crate::process_util::{configure_hidden, ps_base_args};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub timeout: Duration,
    pub output_limit: usize,
}

impl ProcessSpec {
    pub fn new(
        program: impl Into<OsString>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        timeout: Duration,
        output_limit: usize,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            timeout,
            output_limit,
        }
    }

    pub fn powershell(script: &str, timeout: Duration, output_limit: usize) -> Self {
        Self::powershell_with_program("powershell", script, timeout, output_limit)
    }

    pub fn powershell_with_program(
        program: &str,
        script: &str,
        timeout: Duration,
        output_limit: usize,
    ) -> Self {
        let mut args: Vec<OsString> = ps_base_args(program)
            .into_iter()
            .map(OsString::from)
            .collect();
        args.push(OsString::from("-Command"));
        args.push(OsString::from(script));
        Self {
            program: OsString::from(program),
            args,
            timeout,
            output_limit,
        }
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        configure_hidden(&mut command);
        command
    }
}

#[derive(Debug)]
pub struct ProcessResult {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn run(spec: ProcessSpec) -> DstResult<ProcessResult> {
    let mut command = spec.command();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| DstError::Other(format!("启动进程失败：{error}")))?;
    let output_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_reader = child
        .stdout
        .take()
        .map(|stream| spawn_reader(stream, spec.output_limit, Arc::clone(&output_exceeded)))
        .ok_or_else(|| DstError::Other("无法捕获子进程标准输出".into()))?;
    let stderr_reader = child
        .stderr
        .take()
        .map(|stream| spawn_reader(stream, spec.output_limit, Arc::clone(&output_exceeded)))
        .ok_or_else(|| DstError::Other("无法捕获子进程错误输出".into()))?;

    let deadline = Instant::now() + spec.timeout;
    let mut timed_out = false;
    let mut overflow = false;
    let status = loop {
        if output_exceeded.load(Ordering::SeqCst) {
            overflow = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    if timed_out {
        return Err(DstError::Other(format!(
            "进程运行超时（{} ms）：{}",
            spec.timeout.as_millis(),
            program_display(&spec.program)
        )));
    }
    if overflow {
        return Err(DstError::Other(format!(
            "进程输出超过限制（{} 字节）：{}",
            spec.output_limit,
            program_display(&spec.program)
        )));
    }
    let status = status.ok_or_else(|| DstError::Other("进程未返回退出状态".into()))?;
    Ok(ProcessResult {
        status,
        stdout,
        stderr,
    })
}

fn spawn_reader(
    mut stream: impl Read + Send + 'static,
    output_limit: usize,
    exceeded: Arc<AtomicBool>,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            if output.len() + read > output_limit {
                let remaining = output_limit.saturating_sub(output.len());
                output.extend_from_slice(&buffer[..remaining]);
                exceeded.store(true, Ordering::SeqCst);
                break;
            }
            output.extend_from_slice(&buffer[..read]);
        }
        Ok(output)
    })
}

fn join_reader(handle: std::thread::JoinHandle<std::io::Result<Vec<u8>>>) -> DstResult<Vec<u8>> {
    handle
        .join()
        .map_err(|_| DstError::Other("读取子进程输出线程异常".into()))?
        .map_err(DstError::from)
}

fn program_display(program: &OsStr) -> String {
    program.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn long_running_process_is_terminated() {
        let result = run(ProcessSpec::powershell(
            "Start-Sleep -Seconds 30",
            Duration::from_millis(200),
            1024,
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("超时"));
    }

    #[test]
    fn oversized_output_is_bounded() {
        let result = run(ProcessSpec::powershell(
            "'x' * 100000",
            Duration::from_secs(5),
            1024,
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("输出"));
    }
}
