fn main() {
    // Embed Windows resources (application icon, version info).
    // embed_resource detects the target platform automatically;
    // this is a no-op when not targeting Windows.
    let _ = embed_resource::compile("resources/dnf.rc", embed_resource::NONE);
}
