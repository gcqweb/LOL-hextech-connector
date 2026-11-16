//! 单实例守卫：重复启动时激活已存在的窗口并退出新进程。
//!
//! 通过命名互斥体判断是否已有实例在运行；若已有实例，则找到其主窗口
//! 并还原 / 置前，随后让新进程直接退出。

#[cfg(windows)]
mod imp {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;

    const ERROR_ALREADY_EXISTS: u32 = 183;
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;
    const VK_MENU: u8 = 0x12; // Alt
    const KEYEVENTF_KEYUP: u32 = 0x0002;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attrs: *mut c_void, initial_owner: i32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(h: *mut c_void) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> *mut c_void;
        fn SetForegroundWindow(h: *mut c_void) -> i32;
        fn ShowWindow(h: *mut c_void, cmd_show: i32) -> i32;
        fn IsIconic(h: *mut c_void) -> i32;
        fn BringWindowToTop(h: *mut c_void) -> i32;
        fn keybd_event(vk: u8, scan: u8, flags: u32, extra_info: usize);
    }

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// 将已存在实例的主窗口还原并置前。
    fn activate_existing(title: &str) {
        unsafe {
            let t = to_wide(title);
            let hwnd = FindWindowW(std::ptr::null(), t.as_ptr());
            if hwnd.is_null() {
                return;
            }
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            } else {
                ShowWindow(hwnd, SW_SHOW);
            }
            // 模拟一次 Alt 键以绕过 Windows 前台锁定，确保能真正置前。
            keybd_event(VK_MENU, 0, 0, 0);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            keybd_event(VK_MENU, 0, KEYEVENTF_KEYUP, 0);
        }
    }

    /// 返回 `true` 表示当前是首个实例（应继续运行）；
    /// 返回 `false` 表示已有实例在运行（已尝试激活其窗口，新进程应退出）。
    pub fn ensure_single_instance(title: &str) -> bool {
        guard(title, "Local\\HextechConnector_SingleInstance")
    }

    /// 测试专用：以指定互斥体名运行守卫，便于用唯一名称隔离测试，避免与真实实例冲突。
    #[cfg(test)]
    pub fn ensure_single_instance_named(title: &str, mutex_name: &str) -> bool {
        guard(title, mutex_name)
    }

    fn guard(title: &str, mutex_name: &str) -> bool {
        unsafe {
            let name = to_wide(mutex_name);
            let handle = CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
            if handle.is_null() {
                // 创建失败时不阻塞启动（fail-open）。
                return true;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                activate_existing(title);
                return false;
            }
            // 首个实例：保留互斥体句柄至进程结束（不 CloseHandle），进程退出时由系统释放。
            let _ = handle;
            true
        }
    }
}

#[cfg(windows)]
pub use imp::ensure_single_instance;
#[cfg(all(test, windows))]
pub use imp::ensure_single_instance_named;

#[cfg(not(windows))]
pub fn ensure_single_instance(_title: &str) -> bool {
    true
}

#[cfg(all(test, windows))]
mod tests {
    use super::ensure_single_instance_named;

    /// 同一进程内验证命名互斥体检测：首次创建返回 true，再次创建检测到已存在返回 false。
    /// 使用进程唯一的互斥体名与窗口标题，完全隔离，不会触碰真实应用实例。
    #[test]
    fn mutex_detects_existing_instance() {
        let pid = std::process::id();
        let mutex = format!("Local\\HextechConnector_SingleInstance_test_{}", pid);
        let title = format!("海克斯 • 连接器 • single-instance-test-{}", pid);
        assert!(ensure_single_instance_named(&title, &mutex), "首次应判定为首个实例");
        assert!(!ensure_single_instance_named(&title, &mutex), "再次应检测到已存在实例");
    }
}