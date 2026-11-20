fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        res.set("ProductName", "AutoPara");
        res.set("FileDescription", "AutoPara — auto-join calls & classes");
        let _ = res.compile();
    }
}
