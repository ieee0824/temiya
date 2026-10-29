//! OS bounds apply only to disposable Test262 workers, never the parent Engine.
use std::{io, process::Command};

// Cargo derives this from actual instrumentation flags, never runtime env.
// ASan maps terabytes of sparse shadow address space.
pub const ADDRESS_SANITIZED: bool = cfg!(temiya_address_sanitizer);

#[derive(Clone, Copy)]
pub struct Limits {
    pub memory_mb: u64,
    pub stack_kb: u64,
}

impl Limits {
    pub fn memory_bytes(self) -> u64 {
        self.memory_mb * 1024 * 1024
    }

    pub fn address_space_bytes(self) -> Option<u64> {
        (!ADDRESS_SANITIZED).then(|| self.memory_bytes())
    }

    pub fn stack_bytes(self) -> u64 {
        self.stack_kb * 1024
    }

    #[cfg(target_os = "linux")]
    pub fn configure(self, command: &mut Command) -> io::Result<()> {
        use std::os::unix::process::CommandExt;
        let memory = libc::rlim_t::try_from(self.memory_bytes())
            .map_err(|_| io::Error::other("memory limit exceeds platform range"))?;
        let stack = libc::rlim_t::try_from(self.stack_bytes())
            .map_err(|_| io::Error::other("stack limit exceeds platform range"))?;
        // SAFETY: the child hook uses only setrlimit and errno access, without
        // allocation, locks, or inherited Rust state, before exec. Each limit
        // is imposed on the disposable child; the parent is never modified.
        unsafe {
            command.pre_exec(move || {
                for (resource, value) in [
                    (libc::RLIMIT_AS, memory),
                    (libc::RLIMIT_STACK, stack),
                    (libc::RLIMIT_CORE, 0),
                ] {
                    // ASan still has RSS, wall, stack and core bounds.
                    if resource == libc::RLIMIT_AS && ADDRESS_SANITIZED {
                        continue;
                    }
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn configure(self, _: &mut Command) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Test262 process resource limits currently require Linux",
        ))
    }
}

#[cfg(target_os = "linux")]
pub fn resident_kib(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

#[cfg(not(target_os = "linux"))]
pub fn resident_kib(_: u32) -> Option<u64> {
    None
}

pub fn signal(status: std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}
