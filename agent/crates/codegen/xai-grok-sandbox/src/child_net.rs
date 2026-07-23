//! Per-child network isolation.
//!
//! Linux installs a seccomp filter in `pre_exec`. macOS cannot apply an
//! equivalent filter there, so [`child_command`] wraps the target in
//! `/usr/bin/sandbox-exec` with a network-denying Seatbelt profile.

use std::ffi::OsStr;
use std::process::Command;

#[cfg(target_os = "macos")]
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

#[cfg(target_os = "macos")]
const NETWORK_DENY_PROFILE: &str = "(version 1)(allow default)(deny network*)";

/// Build a child command with the platform's network restriction applied.
///
/// On macOS, restricted commands are executed through `sandbox-exec`. On
/// Linux the command remains unchanged because the seccomp filter is installed
/// separately in `pre_exec`; other platforms currently leave it unchanged.
pub fn child_command(program: impl AsRef<OsStr>, restrict_network: bool) -> Command {
    #[cfg(target_os = "macos")]
    if restrict_network {
        let mut command = Command::new(SANDBOX_EXEC);
        command
            .arg("-p")
            .arg(NETWORK_DENY_PROFILE)
            .arg("--")
            .arg(program.as_ref());
        return command;
    }

    let _ = restrict_network;
    Command::new(program)
}

/// Install seccomp BPF filter blocking network syscalls.
///
/// # Safety
///
/// Must be called in a `pre_exec` context (after `fork`, before `exec`).
#[cfg(target_os = "linux")]
pub unsafe fn install_child_network_filter() -> std::io::Result<()> {
    use libc::{
        BPF_ABS, BPF_JEQ, BPF_JMP, BPF_K, BPF_LD, BPF_RET, BPF_W, PR_SET_NO_NEW_PRIVS,
        PR_SET_SECCOMP, SECCOMP_MODE_FILTER, SYS_accept, SYS_accept4, SYS_bind, SYS_connect,
        SYS_listen, SYS_sendmsg, SYS_sendto, prctl, sock_filter, sock_fprog,
    };

    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const EPERM_VAL: u32 = 1; // libc::EPERM

    macro_rules! bpf_stmt {
        ($code:expr, $k:expr) => {
            sock_filter {
                code: $code as u16,
                jt: 0,
                jf: 0,
                k: $k as u32,
            }
        };
    }

    macro_rules! bpf_jump {
        ($code:expr, $k:expr, $jt:expr, $jf:expr) => {
            sock_filter {
                code: $code as u16,
                jt: $jt,
                jf: $jf,
                k: $k as u32,
            }
        };
    }

    const NR_OFFSET: u32 = 0; // seccomp_data.nr offset

    let blocked_syscalls: &[i64] = &[
        SYS_connect,
        SYS_bind,
        SYS_sendto,
        SYS_sendmsg,
        SYS_listen,
        SYS_accept,
        SYS_accept4,
    ];

    let mut filter: Vec<sock_filter> = Vec::new();
    let total_checks = blocked_syscalls.len();

    // 1. Load syscall number
    filter.push(bpf_stmt!(BPF_LD | BPF_W | BPF_ABS, NR_OFFSET));

    // 2. Check each blocked syscall
    for (i, &syscall) in blocked_syscalls.iter().enumerate() {
        let remaining = total_checks - i - 1;
        filter.push(bpf_jump!(
            BPF_JMP | BPF_JEQ | BPF_K,
            syscall,
            remaining as u8 + 1, // match: jump to ERRNO
            0                    // no match: check next
        ));
    }

    // 3. Default: ALLOW
    filter.push(bpf_stmt!(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));

    // 4. Blocked: ERRNO(EPERM)
    filter.push(bpf_stmt!(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | EPERM_VAL));

    let prog = sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };

    // Must set PR_SET_NO_NEW_PRIVS before applying seccomp filter
    // SAFETY: prctl with PR_SET_NO_NEW_PRIVS is safe in pre_exec context.
    if unsafe { prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: prog is a valid sock_fprog pointing to our filter array.
    if unsafe {
        prctl(
            PR_SET_SECCOMP,
            SECCOMP_MODE_FILTER as libc::c_ulong,
            &prog as *const _ as libc::c_ulong,
            0,
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

/// macOS must use [`child_command`] so `sandbox-exec` can wrap the real target.
/// Failing here prevents a future `pre_exec` caller from silently running with
/// unrestricted network access.
///
/// # Safety
///
/// This function must not be used on macOS; use [`child_command`] instead.
#[cfg(target_os = "macos")]
pub unsafe fn install_child_network_filter() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "macOS child network isolation requires child_command/sandbox-exec",
    ))
}

/// No process-level child network filter is available on this platform.
///
/// # Safety
///
/// No-op outside Linux and macOS.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub unsafe fn install_child_network_filter() -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrestricted_command_executes_target_directly() {
        let command = child_command("/bin/echo", false);
        assert_eq!(command.get_program(), "/bin/echo");
        assert_eq!(command.get_args().count(), 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn restricted_macos_command_uses_sandbox_exec() {
        let command = child_command("/bin/echo", true);
        let args: Vec<_> = command.get_args().collect();

        assert_eq!(command.get_program(), SANDBOX_EXEC);
        assert_eq!(
            args,
            ["-p", NETWORK_DENY_PROFILE, "--", "/bin/echo"]
                .map(OsStr::new)
                .as_slice()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sandbox_exec_blocks_child_network_connect() {
        let test_binary = std::env::current_exe().expect("current test binary");
        let mut command = child_command(test_binary, true);
        command
            .args([
                "--exact",
                "child_net::tests::network_connect_probe",
                "--nocapture",
            ])
            .env("XAI_GROK_SANDBOX_NETWORK_PROBE", "1");

        let output = command.output().expect("sandbox-exec should start");
        assert!(
            output.status.success(),
            "network connect must fail with EPERM under sandbox-exec: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn network_connect_probe() {
        if std::env::var_os("XAI_GROK_SANDBOX_NETWORK_PROBE").is_none() {
            return;
        }

        let error =
            std::net::TcpStream::connect("127.0.0.1:9").expect_err("sandboxed connect must fail");
        assert_eq!(error.raw_os_error(), Some(libc::EPERM));
    }
}
