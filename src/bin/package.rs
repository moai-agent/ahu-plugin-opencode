//! Explicit development build/package command. Installation never executes it.
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn project_storage(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let state = root.join(".ahu");
    if !path.starts_with(&state) {
        return Ok(());
    }
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("project storage cannot contain '..'".into());
    }
    match fs::symlink_metadata(&state) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new().mode(0o700).create(&state)?;
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let metadata = fs::symlink_metadata(&state)?;
    // SAFETY: geteuid has no preconditions.
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(".ahu must be a real owner-only directory".into());
    }
    let mut cursor = state;
    for component in path.strip_prefix(&cursor)?.components() {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err("project storage cannot traverse files or symlinks".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if !Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["check-ignore", "--quiet", "--no-index", "--"])
        .arg(path)
        .status()?
        .success()
    {
        return Err("ignore project-local .ahu storage before packaging".into());
    }
    Ok(())
}

fn package() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err(
            "usage: cargo run --locked --bin package -- /absolute/new/dist-directory".into(),
        );
    }
    let destination = PathBuf::from(&args[0]);
    if !destination.is_absolute() || destination.exists() {
        return Err("destination must be absolute and must not already exist".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    project_storage(root, &destination)?;
    let parent = destination
        .parent()
        .ok_or("destination needs a parent")?
        .canonicalize()?;
    let resolved = parent.join(destination.file_name().ok_or("destination needs a name")?);
    if resolved.starts_with(root.canonicalize()?)
        && !resolved.starts_with(root.join("dist"))
        && !resolved.starts_with(root.join(".ahu"))
    {
        return Err("in-checkout output requires ignored .ahu/ or legacy dist/".into());
    }
    project_storage(root, &resolved)?;
    let destination = resolved;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".ahu/build/packages"));
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    project_storage(root, &target)?;
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target)
        .args([
            "build",
            "--release",
            "--locked",
            "--bin",
            "ahu-plugin-opencode",
        ])
        .status()?;
    if !status.success() {
        return Err("release build failed".into());
    }
    fs::create_dir(&destination)?;
    let mut hashes = String::new();
    {
        let (id, source, binary) = ("harness-opencode", ".", "ahu-plugin-opencode");
        let package = destination.join(id);
        fs::create_dir_all(package.join("bin"))?;
        let executable = target.join("release").join(binary);
        fs::copy(&executable, package.join("bin").join(binary))?;
        fs::copy(
            root.join(source).join("ahu-plugin.md"),
            package.join("ahu-plugin.md"),
        )?;
        // This is package identity metadata, not an installation-time build recipe.
        let manifest = fs::read_to_string(root.join(source).join("Cargo.toml"))?;
        let identity = manifest
            .split("[workspace]")
            .next()
            .ok_or("missing package identity")?;
        fs::write(package.join("Cargo.toml"), identity)?;
        for document in [
            "LICENSE",
            "README.md",
            "INSTALL.md",
            "RELEASING.md",
            "SECURITY.md",
            "COMPATIBILITY.md",
            "EXTRACTION.md",
        ] {
            fs::copy(root.join(document), package.join(document))?;
        }
        fs::create_dir(package.join("vendor"))?;
        fs::copy(
            root.join("vendor/PROVENANCE.json"),
            package.join("vendor/PROVENANCE.json"),
        )?;
        let bytes = fs::read(executable)?;
        hashes.push_str(&format!("{:x}  {id}/bin/{binary}\n", Sha256::digest(bytes)));
        println!("{}", package.display());
    }
    fs::write(destination.join("SHA256SUMS"), hashes)?;
    Ok(())
}
fn main() {
    if let Err(error) = package() {
        eprintln!("package: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Checkout(PathBuf);
    impl Checkout {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "ahu-package-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            assert!(
                Command::new("git")
                    .args(["init", "-q"])
                    .arg(&root)
                    .status()
                    .unwrap()
                    .success()
            );
            fs::write(root.join(".gitignore"), "/.ahu/\n").unwrap();
            Self(root.canonicalize().unwrap())
        }
    }
    impl Drop for Checkout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn project_packages_and_builds_use_ignored_private_storage() {
        let checkout = Checkout::new();
        for relative in [".ahu/plugin-packages/new", ".ahu/build/packages"] {
            project_storage(&checkout.0, &checkout.0.join(relative)).unwrap();
        }
        assert_eq!(
            fs::metadata(checkout.0.join(".ahu"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        fs::write(checkout.0.join(".gitignore"), "").unwrap();
        assert!(project_storage(&checkout.0, &checkout.0.join(".ahu/build/packages")).is_err());
    }

    #[test]
    fn project_storage_refuses_redirected_or_public_state() {
        let checkout = Checkout::new();
        let target = checkout.0.join("outside");
        fs::create_dir(&target).unwrap();
        symlink(&target, checkout.0.join(".ahu")).unwrap();
        assert!(project_storage(&checkout.0, &checkout.0.join(".ahu/build/packages")).is_err());
        fs::remove_file(checkout.0.join(".ahu")).unwrap();
        project_storage(&checkout.0, &checkout.0.join(".ahu/build/packages")).unwrap();
        symlink(&target, checkout.0.join(".ahu/build")).unwrap();
        assert!(project_storage(&checkout.0, &checkout.0.join(".ahu/build/packages")).is_err());
        assert!(project_storage(&checkout.0, &checkout.0.join(".ahu/../outside")).is_err());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        fs::remove_file(checkout.0.join(".ahu/build")).unwrap();
        fs::set_permissions(checkout.0.join(".ahu"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(project_storage(&checkout.0, &checkout.0.join(".ahu/build/packages")).is_err());
    }
}
