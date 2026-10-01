//! SHA256 与规范化 JSON（UTF-8、键排序、十进制字符串）内容寻址。
//!
//! 架构约定：确定性内容哈希只含规范化研究内容；run_id、任务时间、导出路径
//! 属于执行元数据，绝不进入哈希输入。

use serde::Serialize;
use sha2::{Digest, Sha256};

/// 字节内容的 SHA256（小写十六进制）。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}

/// 文件内容的 SHA256（小写十六进制）。
pub fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 递归排序对象键，输出无空白紧凑 JSON。
fn write_sorted(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut first = true;
            for k in keys {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_sorted(&map[k], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(arr) => {
            out.push('[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_sorted(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

/// 任意可序列化值 → 规范化 JSON 字符串。
pub fn canonical_json<T: Serialize>(value: &T) -> String {
    let v = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    let mut s = String::new();
    write_sorted(&v, &mut s);
    s
}

/// 任意可序列化值 → 规范化 JSON 的 SHA256。
pub fn hash_canonical<T: Serialize>(value: &T) -> String {
    sha256_hex(canonical_json(value).as_bytes())
}
