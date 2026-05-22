//! Excel 文件读取（calamine 封装）

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use calamine::{open_workbook, Data, Reader, Xlsx};

use crate::error::{AppError, AppResult};

/// Excel 读取器
pub struct ExcelReader<RS: std::io::Read + std::io::Seek> {
    workbook: Xlsx<RS>,
}

/// Sheet 数据 (rows[0] 即 Excel 第 1 行，与 VBA Row 编号一致)
#[derive(Debug, Clone)]
pub struct SheetData {
    pub rows: Vec<Vec<String>>,
    pub dimensions: (usize, usize),
}

impl ExcelReader<BufReader<File>> {
    /// 打开 .xlsx 文件
    pub fn open(path: &Path) -> AppResult<Self> {
        let workbook: Xlsx<_> = open_workbook(path)?;
        Ok(Self { workbook })
    }
}

impl<RS: std::io::Read + std::io::Seek> ExcelReader<RS> {
    /// 获取所有 Sheet 名称
    pub fn sheet_names(&self) -> Vec<String> {
        self.workbook.sheet_names().to_vec()
    }

    /// 读取指定 Sheet 为二维字符串矩阵
    pub fn read_sheet(&mut self, name: &str) -> AppResult<SheetData> {
        let range = self
            .workbook
            .worksheet_range(name)
            .map_err(|_| AppError::SheetNotFound {
                file: "unknown".into(),
                sheet: name.into(),
            })?;

        // 所有行转为字符串向量（不跳过首行，保持与 VBA Row 编号一致）
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|row| row.iter().map(|c| cell_to_string(c)).collect())
            .collect();

        let row_count = rows.len();
        let col_count = rows.first().map(|r| r.len()).unwrap_or(0);

        Ok(SheetData {
            rows,
            dimensions: (row_count, col_count),
        })
    }

    /// 在 Sheet 中搜索关键词，返回 (行索引, 列索引)
    pub fn find_keyword(&mut self, sheet: &str, keywords: &[&str]) -> AppResult<Vec<(usize, usize)>> {
        let range = self
            .workbook
            .worksheet_range(sheet)
            .map_err(|_| AppError::SheetNotFound {
                file: "unknown".into(),
                sheet: sheet.into(),
            })?;

        let mut results = Vec::new();
        for (row_idx, row) in range.rows().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                let text = cell_to_string(cell);
                for kw in keywords {
                    if text.contains(kw) {
                        results.push((row_idx, col_idx));
                    }
                }
            }
        }
        Ok(results)
    }

    /// 读取指定单元格的值
    pub fn read_cell(&mut self, sheet: &str, row: usize, col: usize) -> AppResult<String> {
        let range = self
            .workbook
            .worksheet_range(sheet)
            .map_err(|_| AppError::SheetNotFound {
                file: "unknown".into(),
                sheet: sheet.into(),
            })?;

        if let Some(row_data) = range.rows().nth(row) {
            if let Some(cell) = row_data.get(col) {
                return Ok(cell_to_string(cell));
            }
        }
        Ok(String::new())
    }

    /// 读取指定行
    pub fn read_row(&mut self, sheet: &str, row: usize) -> AppResult<Vec<String>> {
        let range = self
            .workbook
            .worksheet_range(sheet)
            .map_err(|_| AppError::SheetNotFound {
                file: "unknown".into(),
                sheet: sheet.into(),
            })?;

        if let Some(row_data) = range.rows().nth(row) {
            return Ok(row_data.iter().map(cell_to_string).collect());
        }
        Ok(vec![])
    }
}

/// 将 calamine 的 Data 类型转为 String
fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if *f == f64::INFINITY || *f == f64::NEG_INFINITY || f.is_nan() {
                String::new()
            } else if *f == (*f as i64) as f64 && *f < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => format!("{}", i),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        Data::Error(e) => e.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
    }
}
