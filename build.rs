#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    // 设置应用程序图标 (可选，如果你有 icon.ico 文件放在根目录)
    res.set_icon("icon.ico");
    
    // 关键：请求管理员权限
    res.set_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#);
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {}