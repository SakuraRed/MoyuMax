//! 存储概览:受管目录的占用统计。
//!
//! 实例占用按 instances/ 递归统计(跳过符号链接,防环也避免把共享链接
//! 重复计入);备份、Java、回收站占用沿用各自清单 API 的累计值。

use std::{fs, path::Path};

use serde::Serialize;

use crate::{AppService, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageOverview {
    /// 全部实例目录的占用总量(实例根目录下的一切文件)。
    pub instances_bytes: u64,
}

impl AppService {
    /// 存储概览:实例目录占用。instances/ 不存在时视为 0。
    pub fn storage_overview(&self) -> Result<StorageOverview> {
        let instances = self.selected_data_directory()?.join("instances");
        Ok(StorageOverview {
            instances_bytes: directory_size(&instances),
        })
    }
}

fn directory_size(root: &Path) -> u64 {
    fn walk(dir: &Path, total: &mut u64) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                walk(&entry.path(), total);
            } else {
                *total = total.saturating_add(metadata.len());
            }
        }
    }
    let mut total = 0;
    walk(root, &mut total);
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_size_sums_files_recursively_and_skips_missing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("a/b");
        fs::create_dir_all(&nested).expect("mkdir");
        fs::write(nested.join("one.bin"), vec![0_u8; 100]).expect("write");
        fs::write(temp.path().join("two.bin"), vec![0_u8; 23]).expect("write");
        assert_eq!(directory_size(temp.path()), 123);
        assert_eq!(directory_size(&temp.path().join("missing")), 0);
    }
}
