use crate::cli::{BackupArgs, RxArgs, TxArgs};
use crate::config::Config;
use baidu_pcs_rs_sdk::baidu_pcs_sdk::pcs::{BaiduPcsClient, PcsUploadPolicy};
use baidu_pcs_rs_sdk::baidu_pcs_sdk::{PcsFileItem, PcsFileUploadResult};
use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{error::Error, fs};
use tokio_util::either::Either;
use tokio_util::either::Either::{Left, Right};

pub struct LocalSyncFileManager {
    pub path: String,
    pub size: u64,
    pub md5: String,
}

impl LocalSyncFileManager {
    pub fn is_file_has_synced(&self, _path: &Path) -> bool {
        false
    }
}

pub fn scan_files_recursive(dir: &str, mut files: Vec<String>) -> Vec<String> {
    fn is_path_hidden(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("."))
            .unwrap_or(false)
    }
    if !Path::new(dir).exists() {
        return files;
    }
    if Path::new(dir).is_file() {
        let path = fs::canonicalize(PathBuf::from(dir));
        match path.is_ok() {
            true => {
                files.push(path.unwrap().to_string_lossy().to_string());
            }
            false => {
                files.push(dir.to_string());
            }
        }
        return files;
    }
    let paths = fs::read_dir(dir).unwrap();
    for path in paths {
        let path = path.unwrap().path();
        if path.is_dir() && !is_path_hidden(&path) {
            files.append(&mut scan_files_recursive(path.to_str().unwrap(), vec![]));
        } else if path.is_file() && !is_path_hidden(&path) {
            files.push(path.to_str().unwrap().to_string());
        }
    }
    files
}

pub fn task_scheduler<F>(dir: &str, remote_dir: &str, include_prefix: bool, consumer: F)
where
    F: Fn(String, String) -> Result<PcsFileUploadResult, Box<dyn Error>>,
{
    let local_path = PathBuf::from(dir).canonicalize().unwrap();
    let scanned_local_files = if local_path.is_dir() {
        scan_files_recursive(dir, vec![])
    } else {
        vec![local_path.to_string_lossy().to_string()]
    };
    info!("{:?}", scanned_local_files);
    for file in scanned_local_files {
        let pcs_path_buf = PathBuf::from(remote_dir);
        let file_path = PathBuf::from(file.clone());
        let remote_file_path = pcs_path_buf.join(if include_prefix {
            file_path.strip_prefix("/").unwrap()
        } else if local_path.is_absolute() {
            file_path
                .strip_prefix(local_path.parent().unwrap())
                .unwrap()
        } else {
            file_path.as_path()
        });
        info!("{:?}", remote_file_path);
        let _ = consumer(file, remote_file_path.to_string_lossy().to_string());
    }
}

pub(crate) fn run_upload_task(args: &TxArgs, _config: &Config, client: &BaiduPcsClient) {
    let local_root = &args.local;
    let remote_root = &args.remote;
    let remove_source = args.remove_source;
    task_scheduler(
        local_root.as_str(),
        remote_root.as_str(),
        false,
        move |local: String, remote: String| {
            let file_size = fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
            let pb = ProgressBar::new(file_size);
            pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:72.cyan/blue}] {bytes}/{total_bytes} ({percent}%) {bytes_per_sec} ETA {eta_precise} | {msg}", )
                             .unwrap()
                             .progress_chars("=>-"));
            pb.set_message(format!("{} -> {}", local, remote));
            let result = client.upload_large_file(
                local.as_str(),
                remote.as_str(),
                PcsUploadPolicy::Overwrite,
                {
                    let pb = pb.clone();
                    move |p| {
                        // 保障长度一致
                        if pb.length().unwrap_or(0) != p.total_bytes {
                            pb.set_length(p.total_bytes);
                        }
                        pb.set_position(p.uploaded_bytes);
                    }
                },
            );
            match result {
                Ok(result) => {
                    pb.finish_with_message("上传完成");
                    if remove_source {
                        if let Err(e) = fs::remove_file(&local) {
                            error!("删除本地文件失败: {} - {}", local, e);
                            eprintln!("警告: 上传成功但删除本地文件失败: {}", local);
                        } else {
                            info!("已删除本地文件: {}", local);
                        }
                    }
                    Ok(result)
                }
                Err(error) => {
                    pb.abandon_with_message("上传失败");
                    error!("error: {:?}", error);
                    Err(Box::new(error))
                }
            }
        },
    );
}

// 将 name 和 path 组合成一个完整的路径，只保留 name中的不含 / 的最后的部分
// 例如 name = "a/b/c.txt" path = "/d/e/" -> "/d/e/c.txt"
fn get_local_path(name: &str, path: Option<&String>) -> String {
    let name_path = PathBuf::from(name);
    let file_name = name_path
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or(name);
    let path_buf = PathBuf::from(path.unwrap_or(&"./".to_string()));
    let full_path = path_buf.join(file_name);
    full_path.to_string_lossy().to_string()
}

pub(crate) fn run_download_task(args: &RxArgs, _config: &Config, client: &BaiduPcsClient) {
    // 获取远程文件信息，获得文件大小
    let pb = ProgressBar::no_length();
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:72.cyan/blue}] {bytes}/{total_bytes} ({percent}%) {bytes_per_sec} ETA {eta_precise} | {msg}", )
                     .unwrap()
                     .progress_chars("=>-"));
    pb.set_message(format!(
        "{} -> {}",
        args.remote,
        get_local_path(args.remote.as_str(), args.local.as_ref())
    ));
    match resolve_remote_path(args.remote.as_str(), client) {
        Left(remote_path) => {
            let pbm = pb.clone();

            let result = client.down_file(
                remote_path.as_str(),
                get_local_path(args.remote.as_str(), args.local.as_ref()).as_str(),
                Some(move |downloaded, total| {
                    pbm.set_length(total);
                    pbm.set_position(downloaded);
                }),
            );
            match result {
                Ok(_) => {
                    pb.finish_with_message("下载完成");
                }
                Err(error) => {
                    pb.abandon_with_message(format!("下载失败: {}", error.message));
                    error!("error: {:?}", error);
                }
            }
        }
        Right(files) => {
            if !args.recursive {
                pb.finish_and_clear();
                eprintln!("指定文件夹下载时请使用 -r 参数，将递归下载该目录下的所有文件");
                return;
            }
            for file in files {
                if *file.is_dir() == 1 {
                    info!("跳过目录: {}", file.path());
                    continue;
                }

                let remote_path = file.path();
                let pbm = pb.clone();
                let result = client.down_file_by_id(
                    *file.fs_id(),
                    get_local_path(remote_path, args.local.as_ref()).as_str(),
                    Some(move |downloaded, total| {
                        pbm.set_length(total);
                        pbm.set_position(downloaded);
                    }),
                );
                match result {
                    Ok(_) => {
                        pb.finish_with_message("下载完成");
                    }
                    Err(error) => {
                        pb.abandon_with_message(format!(
                            "下载 {} 失败: {}",
                            file.server_filename(),
                            error.message
                        ));
                        error!("error: {:?}", error);
                    }
                }
            }
        }
    }
}

pub(crate) fn resolve_remote_path(
    remote: &str,
    client: &BaiduPcsClient,
) -> Either<String, Vec<PcsFileItem>> {
    // 在不确定用户指定的 remote 是文件还是目录的情况下，先尝试列出目录
    let list = client.list_dir(remote);
    match list {
        Ok(files) => {
            if files.list().is_empty() {
                // 目录为空，说明可能是文件
                Left(remote.to_string())
            } else {
                // 目录不为空，返回目录下的所有文件路径
                Right(files.list().to_vec())
            }
        }
        Err(_) => {
            // 列出目录失败，说明可能是文件
            Left(remote.to_string())
        }
    }
}

/// 递归列出远程目录下所有文件路径
fn list_remote_files_recursive(client: &BaiduPcsClient, dir: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    if let Ok(files) = client.list_dir(dir) {
        for item in files.list() {
            if *item.is_dir() == 1 {
                let sub = list_remote_files_recursive(client, item.path());
                result.extend(sub);
            } else {
                result.insert(item.path().to_string());
            }
        }
    }
    result
}

/// backup 模式：扫描本地文件，跳过远程已存在的，仅上传缺失的
pub(crate) fn run_backup_task(args: &BackupArgs, client: &BaiduPcsClient) {
    let local_root = &args.local;
    let remote_root = &args.remote;
    let remove_source = args.remove_source;

    let local_path = PathBuf::from(local_root)
        .canonicalize()
        .expect("本地路径不存在");
    let local_base = if local_path.is_dir() {
        local_path.clone()
    } else {
        local_path.parent().unwrap().to_path_buf()
    };

    let scanned = scan_files_recursive(local_root, vec![]);
    if scanned.is_empty() {
        println!("没有找到需要备份的本地文件");
        return;
    }

    // 列出远程目录已有文件
    println!("正在检查远程目录 {} ...", remote_root);
    let existing = list_remote_files_recursive(client, remote_root);

    let mut skipped = 0usize;
    let mut uploaded = 0usize;

    for file in &scanned {
        let file_path = PathBuf::from(file);
        let relative = file_path.strip_prefix(&local_base).unwrap_or(&file_path);
        let remote_path = PathBuf::from(remote_root)
            .join(relative)
            .to_string_lossy()
            .to_string();

        if existing.contains(&remote_path) {
            info!("跳过已存在: {}", remote_path);
            skipped += 1;
            continue;
        }

        let file_size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        let pb = ProgressBar::new(file_size);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:72.cyan/blue}] {bytes}/{total_bytes} ({percent}%) {bytes_per_sec} ETA {eta_precise} | {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        pb.set_message(format!("{} -> {}", file, remote_path));

        let result = client.upload_large_file(
            file.as_str(),
            remote_path.as_str(),
            PcsUploadPolicy::Overwrite,
            {
                let pb = pb.clone();
                move |p| {
                    if pb.length().unwrap_or(0) != p.total_bytes {
                        pb.set_length(p.total_bytes);
                    }
                    pb.set_position(p.uploaded_bytes);
                }
            },
        );
        match result {
            Ok(_) => {
                pb.finish_with_message("上传完成");
                uploaded += 1;
                if remove_source {
                    if let Err(e) = fs::remove_file(file) {
                        error!("删除本地文件失败: {} - {}", file, e);
                        eprintln!("警告: 上传成功但删除本地文件失败: {}", file);
                    } else {
                        info!("已删除本地文件: {}", file);
                    }
                }
            }
            Err(err) => {
                pb.abandon_with_message("上传失败");
                error!("备份失败: {} -> {} : {:?}", file, remote_path, err);
            }
        }
    }
    println!(
        "备份完成: 共 {} 个文件, 上传 {}, 跳过 {}",
        scanned.len(),
        uploaded,
        skipped
    );
}

#[cfg(test)]
mod tests {
    use crate::sync::scan_files_recursive;

    #[test]
    fn test_scan_files_recursive() {
        let files = scan_files_recursive(".", vec![]);
        println!("{:?}", files);
        assert!(!files.is_empty());
    }
}
