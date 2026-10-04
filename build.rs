fn main() {
    #[cfg(target_os = "windows")]
    {
        embed_resource::compile_for(
            "assets/app.rc",
            &[env!("CARGO_PKG_NAME")],
            embed_resource::NONE,
        )
        .manifest_required()
        .unwrap();
    }
}
