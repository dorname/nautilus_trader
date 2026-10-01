//! 内容寻址对象存储（架构「任务与提交协议」「内容哈希」节）。
//!
//! - 对象写入：私有临时文件 → fsync → 同卷原子重命名为 `objects/<前2位>/<hash>`；
//! - 相同内容重复写入复用已登记对象，不重写数据；
//! - 读取前校验哈希，不符返回 CORRUPT_ARTIFACT；
//! - 未被元数据登记的对象（孤儿文件）可回收。

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    error::{ErrorCode, ResearchError, Result},
    hash::sha256_hex,
};

pub struct ObjectStore {
    root: PathBuf,
}

impl ObjectStore {
    pub fn new(workspace: &Path) -> Result<Self> {
        let root = workspace.join("objects");
        fs::create_dir_all(&root).map_err(|e| ResearchError::invalid(format!("创建对象目录失败：{e}")))?;
        Ok(Self { root })
    }

    /// 对象的相对路径（写入 manifest 与 artifact 表）。
    pub fn relative_path(hash: &str) -> String {
        format!("objects/{}/{}", &hash[..2], hash)
    }

    fn absolute(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(hash)
    }

    /// 对象绝对路径（只读访问，例如 Parquet 分区读取）。
    pub fn path_of(&self, hash: &str) -> PathBuf {
        self.absolute(hash)
    }

    /// 写入字节内容，返回内容哈希；已存在则复用，不重写。
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let hash = sha256_hex(bytes);
        let target = self.absolute(&hash);
        if target.exists() {
            return Ok(hash);
        }
        let dir = target.parent().unwrap_or(&self.root).to_path_buf();
        fs::create_dir_all(&dir).map_err(|e| ResearchError::invalid(format!("创建对象分片目录失败：{e}")))?;
        let tmp = dir.join(format!(".{hash}.tmp"));
        fs::write(&tmp, bytes).map_err(|e| map_io("写入对象临时文件", e))?;
        let f = fs::File::open(&tmp).map_err(|e| map_io("打开对象临时文件", e))?;
        f.sync_all().map_err(|e| map_io("同步对象文件", e))?;
        fs::rename(&tmp, &target).map_err(|e| map_io("原子重命名对象", e))?;
        // 只读内容目录：对象创建后不更新
        let mut perms = fs::metadata(&target)
            .map_err(|e| map_io("读取对象元数据", e))?
            .permissions();
        perms.set_readonly(true);
        let _ = fs::set_permissions(&target, perms);
        Ok(hash)
    }

    /// 写入磁盘文件内容（读取后按内容寻址登记），返回内容哈希。
    pub fn put_file(&self, path: &Path) -> Result<String> {
        let bytes = fs::read(path).map_err(|e| map_io("读取待登记文件", e))?;
        self.put(&bytes)
    }

    /// 读取对象并校验哈希。
    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.absolute(hash);
        let bytes = fs::read(&path)
            .map_err(|_| ResearchError::not_found(format!("内容对象不存在：{hash}")))?;
        if sha256_hex(&bytes) != hash {
            return Err(ResearchError::new(
                ErrorCode::CorruptArtifact,
                format!("内容对象哈希不符，停止读取：{hash}"),
            ));
        }
        Ok(bytes)
    }

    /// 回收未被登记哈希集合引用的孤儿对象，返回移除的哈希列表。
    pub fn reclaim_orphans(&self, registered: &HashSet<String>) -> Result<Vec<String>> {
        let mut removed = Vec::new();
        for shard in fs::read_dir(&self.root).map_err(|e| ResearchError::invalid(format!("遍历对象目录失败：{e}")))? {
            let shard = shard.map_err(|e| ResearchError::invalid(format!("读取对象分片失败：{e}")))?;
            if !shard.path().is_dir() {
                continue;
            }
            for entry in fs::read_dir(shard.path()).map_err(|e| ResearchError::invalid(format!("遍历对象分片失败：{e}")))? {
                let entry = entry.map_err(|e| ResearchError::invalid(format!("读取对象条目失败：{e}")))?;
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || registered.contains(&name) {
                    continue;
                }
                // 只读权限需先解除再删除
                if let Ok(meta) = entry.metadata() {
                    let mut perms = meta.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(entry.path(), perms);
                }
                fs::remove_file(entry.path()).map_err(|e| map_io("移除孤儿对象", e))?;
                removed.push(name);
            }
        }
        Ok(removed)
    }
}

fn map_io(context: &str, e: std::io::Error) -> ResearchError {
    if e.raw_os_error() == Some(28) {
        return ResearchError::new(ErrorCode::DiskFull, format!("{context}：磁盘空间不足"));
    }
    ResearchError::invalid(format!("{context}：{e}"))
}
