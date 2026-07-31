use std::{
    fs::{File, OpenOptions},
    io::{self, Read as _, Write as _},
    iter,
    os::windows::{ffi::OsStrExt as _, fs::MetadataExt as _},
    path::Path,
    ptr::{null, null_mut},
    slice,
};

use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, LocalFree},
    Security::{
        ACL,
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
        },
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
    },
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
    },
};

const PRIVATE_FILE_SDDL: &str = "D:P(A;;FA;;;SY)(A;;FA;;;BA)";
const PRIVATE_DIRECTORY_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

#[derive(Clone, Copy)]
enum PathKind {
    File,
    Directory,
}

struct OwnedSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl OwnedSecurityDescriptor {
    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let wide = sddl.encode_utf16().chain(iter::once(0)).collect::<Vec<_>>();
        let mut descriptor = null_mut();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                null_mut(),
            )
        };
        if converted == 0 || descriptor.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for OwnedSecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

/// Creates or hardens a non-reparse directory with the exact private DACL.
///
/// # Errors
///
/// Returns an error for relative or reparse paths, wrong object types, ACL drift, or I/O failure.
pub fn ensure_private_directory(path: &Path) -> io::Result<()> {
    require_absolute(path)?;
    std::fs::create_dir_all(path)?;
    validate_path_chain(path)?;
    validate_kind(path, PathKind::Directory)?;
    apply_private_acl(path, PathKind::Directory)
}

/// Reads a bounded regular file only when its path chain and DACL are private.
///
/// # Errors
///
/// Returns an error for unsafe paths, wrong object types, ACL drift, size changes, or I/O failure.
pub fn read_private(path: &Path, maximum_bytes: u64) -> io::Result<Vec<u8>> {
    require_absolute(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    validate_path_chain(parent)?;
    validate_kind(parent, PathKind::Directory)?;
    verify_private_acl(parent, PathKind::Directory)?;
    let metadata = validate_kind(path, PathKind::File)?;
    if metadata.len() > maximum_bytes {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    verify_private_acl(path, PathKind::File)?;
    let file = File::open(path)?;
    let opened = file.metadata()?;
    validate_metadata(&opened, PathKind::File)?;
    if opened.len() != metadata.len() || opened.len() > maximum_bytes {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let capacity =
        usize::try_from(opened.len()).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()) != Ok(opened.len()) {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok(bytes)
}

/// Atomically replaces a private file using a same-directory restricted temporary file.
///
/// # Errors
///
/// Returns an error for unsafe paths, ACL drift, non-atomic placement, or persistence failure.
pub fn write_private_atomic(path: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    require_absolute(path)?;
    require_absolute(temporary)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    if temporary.parent() != Some(parent) || temporary == path {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    ensure_private_directory(parent)?;
    match path.symlink_metadata() {
        Ok(_) => {
            validate_kind(path, PathKind::File)?;
            verify_private_acl(path, PathKind::File)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)?;
        validate_metadata(&file.metadata()?, PathKind::File)?;
        apply_private_acl(temporary, PathKind::File)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_private_file(path, temporary)?;
        validate_kind(path, PathKind::File)?;
        verify_private_acl(path, PathKind::File)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn require_absolute(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(())
}

fn validate_path_chain(path: &Path) -> io::Result<()> {
    for component in path.ancestors() {
        let metadata = component.symlink_metadata()?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    Ok(())
}

fn validate_kind(path: &Path, kind: PathKind) -> io::Result<std::fs::Metadata> {
    let metadata = path.symlink_metadata()?;
    validate_metadata(&metadata, kind)?;
    Ok(metadata)
}

fn validate_metadata(metadata: &std::fs::Metadata, kind: PathKind) -> io::Result<()> {
    let expected_kind = match kind {
        PathKind::File => metadata.is_file(),
        PathKind::Directory => metadata.is_dir(),
    };
    if !expected_kind || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok(())
}

fn apply_private_acl(path: &Path, kind: PathKind) -> io::Result<()> {
    let expected = expected_descriptor(kind)?;
    let dacl = descriptor_dacl(expected.0)?;
    let wide = wide_path(path)?;
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(win32_error(status));
    }
    verify_private_acl(path, kind)
}

fn verify_private_acl(path: &Path, kind: PathKind) -> io::Result<()> {
    let expected = expected_descriptor(kind)?;
    let expected_acl = acl_bytes(expected.0)?;
    let actual = query_descriptor(path)?;
    let mut control = 0_u16;
    let mut revision = 0_u32;
    let control_ok =
        unsafe { GetSecurityDescriptorControl(actual.0, &raw mut control, &raw mut revision) };
    if control_ok == 0 || control & SE_DACL_PROTECTED == 0 {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    let actual_acl = acl_bytes(actual.0)?;
    if actual_acl != expected_acl {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    Ok(())
}

fn expected_descriptor(kind: PathKind) -> io::Result<OwnedSecurityDescriptor> {
    let sddl = match kind {
        PathKind::File => PRIVATE_FILE_SDDL,
        PathKind::Directory => PRIVATE_DIRECTORY_SDDL,
    };
    OwnedSecurityDescriptor::from_sddl(sddl)
}

fn query_descriptor(path: &Path) -> io::Result<OwnedSecurityDescriptor> {
    let wide = wide_path(path)?;
    let mut descriptor = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS || descriptor.is_null() {
        return Err(win32_error(status));
    }
    Ok(OwnedSecurityDescriptor(descriptor))
}

fn descriptor_dacl(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<*mut ACL> {
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = null_mut();
    let valid = unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &raw mut present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    };
    if valid == 0 || present == 0 || defaulted != 0 || dacl.is_null() {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    Ok(dacl)
}

fn acl_bytes(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<Vec<u8>> {
    let dacl = descriptor_dacl(descriptor)?;
    let length = unsafe { usize::from((*dacl).AclSize) };
    if length < size_of::<ACL>() {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let bytes = unsafe { slice::from_raw_parts(dacl.cast::<u8>(), length) };
    Ok(bytes.to_vec())
}

fn replace_private_file(path: &Path, temporary: &Path) -> io::Result<()> {
    let target = wide_path(path)?;
    let replacement = wide_path(temporary)?;
    let exists = path.symlink_metadata().is_ok();
    let replaced = if exists {
        unsafe {
            ReplaceFileW(
                target.as_ptr(),
                replacement.as_ptr(),
                null(),
                REPLACEFILE_WRITE_THROUGH,
                null(),
                null(),
            )
        }
    } else {
        unsafe {
            MoveFileExW(
                replacement.as_ptr(),
                target.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if replaced == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    if wide.len() <= 1 || wide[..wide.len() - 1].contains(&0) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(wide)
}

fn win32_error(status: u32) -> io::Error {
    i32::try_from(status).map_or_else(
        |_| io::Error::from(io::ErrorKind::Other),
        io::Error::from_raw_os_error,
    )
}
