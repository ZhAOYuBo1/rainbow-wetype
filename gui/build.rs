fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_manifest_file("app.manifest");
    res.set_icon("assets/app.ico");
    res.compile()
        .expect("failed to compile Windows resources / manifest");
}
