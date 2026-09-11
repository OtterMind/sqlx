use std::process::{Child, Command};

pub(crate) fn spawn_detached(command: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200);
    }
    // Redirected child streams do not by themselves prevent Windows inheriting
    // the caller's original output pipes. Clear inheritance during the spawn.
    #[cfg(windows)]
    let _pipes = windows::StandardHandleInheritance::disable()?;
    command.spawn()
}

#[cfg(windows)]
mod windows {
    use std::io;
    use windows_sys::Win32::{
        Foundation::{
            GetHandleInformation, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
            INVALID_HANDLE_VALUE,
        },
        System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
    };

    pub struct StandardHandleInheritance(Vec<HANDLE>);
    impl StandardHandleInheritance {
        pub fn disable() -> io::Result<Self> {
            let mut guard = Self(Vec::new());
            for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                // SAFETY: These are borrowed process standard handles. They remain open
                // while spawning; only their inheritance bit is changed and restored.
                unsafe {
                    let handle = GetStdHandle(stream);
                    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                        continue;
                    }
                    let mut flags = 0;
                    if GetHandleInformation(handle, &mut flags) == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if flags & HANDLE_FLAG_INHERIT != 0 {
                        if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) == 0 {
                            return Err(io::Error::last_os_error());
                        }
                        guard.0.push(handle);
                    }
                }
            }
            Ok(guard)
        }
    }
    impl Drop for StandardHandleInheritance {
        fn drop(&mut self) {
            for &handle in &self.0 {
                // SAFETY: Same borrowed handles as above; no ownership is transferred.
                unsafe {
                    SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
                }
            }
        }
    }
}
