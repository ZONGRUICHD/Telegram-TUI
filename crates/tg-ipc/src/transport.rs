//! Private, local transport: Unix sockets or owner-only Windows named pipes.
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait LocalStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> LocalStream for T {}
pub type Stream = Box<dyn LocalStream>;

pub async fn connect(path: &Path) -> anyhow::Result<Stream> {
    #[cfg(unix)]
    return Ok(Box::new(tokio::net::UnixStream::connect(path).await?));
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        let name = pipe_name(path)?;
        for _ in 0..100 {
            match ClientOptions::new().open(&name) {
                Ok(pipe) => return Ok(Box::new(pipe)),
                Err(e) if e.raw_os_error() == Some(231) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::bail!("本地服务繁忙，请重试")
    }
}

pub struct Listener {
    path: PathBuf,
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    inner: tokio::net::windows::named_pipe::NamedPipeServer,
}

impl Listener {
    /// Caller must hold the endpoint's exclusive daemon lock before binding.
    pub fn bind(path: &Path) -> anyhow::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{FileTypeExt, PermissionsExt};
            if let Some(parent) = path.parent() {
                let exists = parent.exists();
                std::fs::create_dir_all(parent)?;
                if !exists {
                    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
                }
            }
            if let Ok(meta) = std::fs::symlink_metadata(path) {
                anyhow::ensure!(
                    meta.file_type().is_socket(),
                    "IPC 路径不是 socket，拒绝覆盖"
                );
                std::fs::remove_file(path)?;
            }
            let inner = tokio::net::UnixListener::bind(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self {
                path: path.to_owned(),
                inner,
            })
        }
        #[cfg(windows)]
        {
            let name = pipe_name(path)?;
            Ok(Self {
                inner: create_pipe(&name, true)?,
                path: name,
            })
        }
    }

    pub async fn accept(&mut self) -> anyhow::Result<Stream> {
        #[cfg(unix)]
        return Ok(Box::new(self.inner.accept().await?.0));
        #[cfg(windows)]
        {
            self.inner.connect().await?;
            let next = create_pipe(&self.path, false)?;
            Ok(Box::new(std::mem::replace(&mut self.inner, next)))
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
fn pipe_name(path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Fixed FNV-1a hash: identical name in independently built tg / tgcd binaries.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in absolute
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase()
        .bytes()
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(PathBuf::from(format!(r"\\.\pipe\telegram-tui-{hash:016x}")))
}

#[cfg(windows)]
fn create_pipe(
    name: &Path,
    first: bool,
) -> anyhow::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use std::ptr::null_mut;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
    };
    // OW is the Windows OWNER RIGHTS SID. Only the object's owner gets access.
    let sddl: Vec<u16> = "D:P(A;;GA;;;OW)\0".encode_utf16().collect();
    let mut descriptor = null_mut();
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let result = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            );
        LocalFree(descriptor);
        Ok(result?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn local_connection_round_trip() {
        let path = std::env::temp_dir()
            .join(format!("tg-{}", uuid::Uuid::new_v4()))
            .join("test.sock");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut listener = Listener::bind(&path).unwrap();
        let client_path = path.clone();
        let task = tokio::spawn(async move {
            let mut stream = connect(&client_path).await.unwrap();
            stream.write_all(b"ping").await.unwrap();
            let mut reply = [0; 4];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"pong");
        });
        let mut stream = listener.accept().await.unwrap();
        let mut request = [0; 4];
        stream.read_exact(&mut request).await.unwrap();
        assert_eq!(&request, b"ping");
        stream.write_all(b"pong").await.unwrap();
        task.await.unwrap();
        drop(stream);
        drop(listener);
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }
}
