# Filesystem Rules

Applies to every module in `src/filesystem/` and to any code that touches the file system.

## API usage

- Use the Rust standard library filesystem APIs.
- Use `Path` and `PathBuf` for all filesystem paths.
- Never construct filesystem paths by manually concatenating `"/"` or `"\\"`. Use `Path::join`, `PathBuf::push` or the appropriate std API.
- Never use shell commands such as `cp`, `mv`, `rm`, `mkdir` or similar for filesystem operations.
- Never execute selected files automatically. Selecting an entry must never run it.
- Do not scan the entire filesystem unless explicitly required.

## Fallibility

Treat every filesystem operation as fallible, and handle at least:

- Permission errors.
- Missing files.
- Files disappearing between listing and operation.
- Invalid paths.
- Read-only filesystems.
- Broken symlinks.

## Filenames

- Preserve Unicode filenames.
- Never assume filenames are ASCII.

## Responsiveness

- Avoid blocking the UI with expensive filesystem work.
