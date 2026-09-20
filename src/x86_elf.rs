use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn link_x86_elf(assembly: &str, output: &Path) -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before unix epoch: {error}"))?
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-x86-elf-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");

    fs::write(&source, assembly).map_err(|error| format!("writing assembly: {error}"))?;
    fs::write(
        &launcher,
        "#include <stdint.h>\nextern uint64_t wsm_entry(void *);\nint main(void) { (void)wsm_entry(0); return 0; }\n",
    )
    .map_err(|error| format!("writing launcher: {error}"))?;

    let nucleus = crate::x86_freestanding::resolve_nucleus_asm_path()?;
    let linked = Command::new("cc")
        .arg(&launcher)
        .arg(&source)
        .arg(nucleus)
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|error| format!("starting linker: {error}"))?;

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&launcher);

    if !linked.status.success() {
        return Err(format!(
            "x86 ELF link failed: {}",
            String::from_utf8_lossy(&linked.stderr)
        ));
    }

    Ok(())
}
