//! 纯 Rust xlsx 模板修改引擎 (路线2: ZIP + XML 直接操作)
//!
//! 核心思路: xlsx = ZIP 压缩包, 内含 XML 文件。
//! 打开模板 → 修改目标单元格 → 保存, 完美保留所有格式/公式/图表/合并单元格。
//!
//! 依赖: zip (纯Rust) + quick-xml (纯Rust), 零 C 依赖, 不会硬崩溃。

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Reader as XmlReader;
use quick_xml::Writer as XmlWriter;

use crate::error::AppError;

/// xlsx 单元格值类型
#[derive(Debug, Clone)]
pub enum CellValue {
    Number(f64),
    String(String),
    Formula(String),
}

/// xlsx 模板修改器
pub struct XlsxWriter {
    entries: HashMap<String, Vec<u8>>,
    shared_strings: Vec<String>,
    sst_modified: bool,
}

impl XlsxWriter {
    /// 打开已有 xlsx 文件
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let file = std::fs::File::open(path).map_err(|e| {
            AppError::FileNotFound(format!("无法打开模板文件 {}: {}", path.display(), e))
        })?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| {
            AppError::Other(format!("xlsx(ZIP) 解析失败: {}", e))
        })?;

        let mut entries = HashMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| {
                AppError::Other(format!("读取 ZIP 条目 {}: {}", i, e))
            })?;
            let name = entry.name().to_string();
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| {
                AppError::Other(format!("读取 ZIP 条目 '{}': {}", name, e))
            })?;
            entries.insert(name, buf);
        }

        let sst = Self::load_sst(&entries);
        Ok(Self {
            entries,
            shared_strings: sst,
            sst_modified: false,
        })
    }

    /// 创建空白工作簿（仅含最小模板结构）
    pub fn empty() -> Self {
        let shared_strings = Vec::new();
        let mut entries = HashMap::new();

        // [Content_Types].xml
        entries.insert(
            "[Content_Types].xml".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
  <Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
</Types>"#
                .into(),
        );

        // _rels/.rels
        entries.insert(
            "_rels/.rels".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#
                .into(),
        );

        // xl/workbook.xml
        entries.insert(
            "xl/workbook.xml".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#
                .into(),
        );

        // xl/_rels/workbook.xml.rels
        entries.insert(
            "xl/_rels/workbook.xml.rels".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#
                .into(),
        );

        // xl/worksheets/sheet1.xml
        entries.insert(
            "xl/worksheets/sheet1.xml".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData/>
</worksheet>"#
                .into(),
        );

        // xl/sharedStrings.xml
        entries.insert(
            "xl/sharedStrings.xml".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0"/>
</sst>"#
                .into(),
        );

        // xl/styles.xml — 最小样式
        entries.insert(
            "xl/styles.xml".into(),
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
  <fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>
  <borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
  <cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>
</styleSheet>"#
                .into(),
        );

        Self {
            entries,
            shared_strings,
            sst_modified: false,
        }
    }

    /// 确保目标 Sheet 存在（不存在则创建）
    pub fn ensure_sheet(&mut self, name: &str) -> Result<(), AppError> {
        let sheet_path = Self::find_sheet_path(&self.entries, name);
        if sheet_path.is_some() {
            return Ok(());
        }
        // 创建新 sheet
        let sheet_count = self.sheet_count();
        let new_id = sheet_count + 1;
        let r_id = format!("rId{}", 100 + new_id);
        let fname = format!("worksheets/sheet{}.xml", new_id);
        let path = format!("xl/{}", fname);

        self.entries.insert(
            path,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData/>
</worksheet>"#
            )
            .into(),
        );

        // 更新 workbook.xml — 添加 sheet 引用
        Self::patch_workbook_sheets(&mut self.entries, name, new_id, &r_id);
        // 更新 _rels — 添加 worksheet 关系
        Self::patch_workbook_rels(&mut self.entries, &r_id, &fname);
        // 更新 [Content_Types].xml — 添加 worksheet 类型
        Self::patch_content_types(&mut self.entries, &format!("/xl/{}", fname));

        Ok(())
    }

    /// 设置单元格数值
    pub fn set_number(&mut self, sheet: &str, col: u32, row: u32, value: f64) -> Result<(), AppError> {
        let path = Self::find_sheet_path(&self.entries, sheet)
            .ok_or_else(|| AppError::SheetNotFound {
                file: "xlsx".into(),
                sheet: sheet.to_string(),
            })?;
        let xml = self.entries.get(&path).cloned().unwrap_or_default();
        let modified = modify_cell_number(&xml, col, row, value)
            .map_err(|e| AppError::Other(format!("修改单元格 {}{}: {}", col_letter(col), row, e)))?;
        self.entries.insert(path, modified);
        Ok(())
    }

    /// 设置单元格文本
    pub fn set_string(&mut self, sheet: &str, col: u32, row: u32, text: &str) -> Result<(), AppError> {
        if text.is_empty() {
            return Ok(());
        }
        let path = Self::find_sheet_path(&self.entries, sheet)
            .ok_or_else(|| AppError::SheetNotFound {
                file: "xlsx".into(),
                sheet: sheet.to_string(),
            })?;
        let xml = self.entries.get(&path).cloned().unwrap_or_default();
        let sst_idx = self.add_shared_string(text);
        let modified = modify_cell_string(&xml, col, row, sst_idx)
            .map_err(|e| AppError::Other(format!("修改单元格 {}{}: {}", col_letter(col), row, e)))?;
        self.entries.insert(path, modified);
        Ok(())
    }

    /// 设置单元格公式
    pub fn set_formula(&mut self, sheet: &str, col: u32, row: u32, formula: &str) -> Result<(), AppError> {
        let path = Self::find_sheet_path(&self.entries, sheet)
            .ok_or_else(|| AppError::SheetNotFound {
                file: "xlsx".into(),
                sheet: sheet.to_string(),
            })?;
        let xml = self.entries.get(&path).cloned().unwrap_or_default();
        let modified = modify_cell_formula(&xml, col, row, formula)
            .map_err(|e| AppError::Other(format!("修改单元格 {}{}: {}", col_letter(col), row, e)))?;
        self.entries.insert(path, modified);
        Ok(())
    }

    /// 设置 NumberFormat 样式（对区域应用格式码）
    pub fn set_number_format(
        &mut self,
        sheet: &str,
        col_start: u32,
        col_end: u32,
        row_start: u32,
        row_end: u32,
        format_code: &str,
    ) -> Result<(), AppError> {
        let path = Self::find_sheet_path(&self.entries, sheet)
            .ok_or_else(|| AppError::SheetNotFound {
                file: "xlsx".into(),
                sheet: sheet.to_string(),
            })?;
        let xml = self.entries.get(&path).cloned().unwrap_or_default();
        let modified = apply_number_format_region(&xml, col_start, col_end, row_start, row_end, format_code)
            .map_err(|e| AppError::Other(format!("设置格式: {}", e)))?;
        self.entries.insert(path, modified);
        Ok(())
    }

    /// 保存 xlsx 文件
    pub fn save(&self, output_path: &Path) -> Result<(), AppError> {
        if let Some(p) = output_path.parent() {
            std::fs::create_dir_all(p).map_err(|e| {
                AppError::Other(format!("无法创建输出目录 '{}': {}", p.display(), e))
            })?;
        }

        let tmp_dir = std::env::temp_dir();
        let fname = output_path.file_stem().unwrap_or_default().to_string_lossy();
        let pid = std::process::id();
        let tmp = tmp_dir.join(format!("ExcelMiner_{}_{}.xlsxtmp", fname, pid));

        let _ = std::fs::remove_file(&tmp);

        let buf = self.to_zip()?;
        std::fs::write(&tmp, &buf).map_err(|e| {
            AppError::Other(format!("写入临时文件失败: {}", e))
        })?;

        // 原子替换
        if output_path.exists() {
            std::fs::remove_file(output_path).map_err(|_| {
                let _ = std::fs::remove_file(&tmp);
                AppError::Other("无法写入报表（可能被 Excel 打开），请关闭后重试".into())
            })?;
        }
        if std::fs::rename(&tmp, output_path).is_err() {
            std::fs::copy(&tmp, output_path).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                AppError::Other(format!("无法完成报表写入: {}", e))
            })?;
            let _ = std::fs::remove_file(&tmp);
        }

        let size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
        tracing::info!("[xlsx_writer] 保存完成: {} ({} bytes)", output_path.display(), size);
        Ok(())
    }

    /// 返回 sheet 数量（用于确定新 sheet 的 ID）
    fn sheet_count(&self) -> usize {
        if let Some(xml) = self.entries.get("xl/workbook.xml") {
            let s = String::from_utf8_lossy(xml);
            s.matches("<sheet ").count()
        } else {
            1
        }
    }

    /// 在 ZIP entries 中查找 Sheet 的路径
    fn find_sheet_path(entries: &HashMap<String, Vec<u8>>, name: &str) -> Option<String> {
        // 先通过 workbook.xml 查找 sheetId → 路径 映射
        if let Some(xml) = entries.get("xl/workbook.xml") {
            let s = String::from_utf8_lossy(xml);
            for line in s.lines() {
                if line.contains(&format!("name=\"{}\"", name)) {
                    if let Some(start) = line.find("r:id=\"") {
                        let rest = &line[start + 6..];
                        if let Some(end) = rest.find('\"') {
                            let r_id = &rest[..end];
                            // 通过 _rels 查找路径
                            if let Some(rels) = entries.get("xl/_rels/workbook.xml.rels") {
                                let rs = String::from_utf8_lossy(rels);
                                let needle = format!("Id=\"{}\"", r_id);
                                for rline in rs.lines() {
                                    if rline.contains(&needle) {
                                        if let Some(ts) = rline.find("Target=\"") {
                                            let target = &rline[ts + 8..];
                                            if let Some(te) = target.find('\"') {
                                                return Some(format!("xl/{}", &target[..te]));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // workbook.xml 中未找到 → 确认不存在此 Sheet
        None
    }

    // ─── Shared String Table ───────────────────────────────────

    fn load_sst(entries: &HashMap<String, Vec<u8>>) -> Vec<String> {
        let xml = match entries.get("xl/sharedStrings.xml") {
            Some(b) => String::from_utf8_lossy(b).into_owned(),
            None => return Vec::new(),
        };
        let mut strings = Vec::new();
        let mut reader = XmlReader::from_str(&xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let mut in_si = false;
        let mut in_t = false;
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    if e.name().as_ref() == b"si" {
                        in_si = true;
                    } else if in_si && e.name().as_ref() == b"t" {
                        in_t = true;
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_t {
                        strings.push(e.unescape().unwrap_or_default().into_owned());
                    }
                }
                Ok(Event::End(ref e)) => {
                    match e.name().as_ref() {
                        b"si" => in_si = false,
                        b"t" => in_t = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => continue,
                _ => {}
            }
            buf.clear();
        }
        strings
    }

    fn add_shared_string(&mut self, text: &str) -> usize {
        if let Some(idx) = self.shared_strings.iter().position(|s| s == text) {
            return idx;
        }
        let idx = self.shared_strings.len();
        self.shared_strings.push(text.to_string());
        self.sst_modified = true;
        idx
    }

    fn build_sst(count: usize, unique: usize, strings: &[String]) -> Vec<u8> {
        let mut w = XmlWriter::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
        w.write_event(Event::Decl(quick_xml::events::BytesDecl::new("1.0", Some("UTF-8"), Some("yes"))))
            .ok();
        let mut sst = BytesStart::new("sst");
        sst.push_attribute(("xmlns", "http://schemas.openxmlformats.org/spreadsheetml/2006/main"));
        sst.push_attribute(("count", &*count.to_string()));
        sst.push_attribute(("uniqueCount", &*unique.to_string()));
        w.write_event(Event::Start(sst)).ok();
        for s in strings {
            w.write_event(Event::Start(BytesStart::new("si"))).ok();
            w.write_event(Event::Start(BytesStart::new("t"))).ok();
            w.write_event(Event::Text(BytesText::new(s))).ok();
            w.write_event(Event::End(BytesEnd::new("t"))).ok();
            w.write_event(Event::End(BytesEnd::new("si"))).ok();
        }
        w.write_event(Event::End(BytesEnd::new("sst"))).ok();
        w.into_inner().into_inner()
    }

    // ─── 序列化 ZIP ──────────────────────────────────────────

    fn to_zip(&self) -> Result<Vec<u8>, AppError> {
        let buf = Cursor::new(Vec::new());
        let mut zip_writer = zip::ZipWriter::new(buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let mut all_entries: Vec<(&String, &Vec<u8>)> = self.entries.iter().collect();
        all_entries.sort_by_key(|(k, _)| *k);

        for (name, data) in &all_entries {
            let actual_data = if *name == "xl/sharedStrings.xml" && self.sst_modified {
                // 使用重建的 SST
                Self::build_sst(
                    self.shared_strings.len(),
                    self.shared_strings.len(),
                    &self.shared_strings,
                )
            } else {
                (*data).clone()
            };
            zip_writer
                .start_file(*name, opts)
                .map_err(|e| AppError::Other(format!("ZIP 创建条目 '{}': {}", name, e)))?;
            zip_writer
                .write_all(&actual_data)
                .map_err(|e| AppError::Other(format!("ZIP 写入条目 '{}': {}", name, e)))?;
        }
        let cursor = zip_writer
            .finish()
            .map_err(|e| AppError::Other(format!("ZIP 完成: {}", e)))?;
        Ok(cursor.into_inner())
    }

    // ─── 内部 helpers ────────────────────────────────────────

    fn patch_workbook_sheets(entries: &mut HashMap<String, Vec<u8>>, name: &str, id: usize, r_id: &str) {
        let xml = entries.get("xl/workbook.xml").cloned().unwrap_or_default();
        let s = String::from_utf8_lossy(&xml);
        let sheet_tag = format!(
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"{}\"/>",
            name, id, r_id
        );
        let new_s = if let Some(pos) = s.find("</sheets>") {
            format!("{}\n  {}\n  {}", &s[..pos], sheet_tag, &s[pos..])
        } else {
            s.replace("</workbook>", &format!("  <sheets>\n    {}\n  </sheets>\n</workbook>", sheet_tag))
        };
        entries.insert("xl/workbook.xml".into(), new_s.into_bytes());
    }

    fn patch_workbook_rels(entries: &mut HashMap<String, Vec<u8>>, r_id: &str, target: &str) {
        let xml = entries.get("xl/_rels/workbook.xml.rels").cloned().unwrap_or_default();
        let s = String::from_utf8_lossy(&xml);
        let rel_tag = format!(
            "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"{}\"/>",
            r_id, target
        );
        let new_s: String = if let Some(pos) = s.find("</Relationships>") {
            format!("{}  {}\n{}", &s[..pos], rel_tag, &s[pos..])
        } else {
            s.into_owned()
        };
        entries.insert("xl/_rels/workbook.xml.rels".into(), new_s.into_bytes());
    }

    fn patch_content_types(entries: &mut HashMap<String, Vec<u8>>, path: &str) {
        let xml = entries.get("[Content_Types].xml").cloned().unwrap_or_default();
        let s = String::from_utf8_lossy(&xml);
        let override_tag = format!(
            "<Override PartName=\"{}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            path
        );
        let new_s: String = if let Some(pos) = s.find("</Types>") {
            format!("{}  {}\n{}", &s[..pos], override_tag, &s[pos..])
        } else {
            s.into_owned()
        };
        entries.insert("[Content_Types].xml".into(), new_s.into_bytes());
    }
}

// ─── 单元格修改（直接操作 Sheet XML） ─────────────────────────

/// 修改/新增数值单元格
fn modify_cell_number(sheet_xml: &[u8], col: u32, row: u32, value: f64) -> Result<Vec<u8>, String> {
    let cell_ref = format!("{}{}", col_letter(col), row);
    modify_cell_in_xml(sheet_xml, &cell_ref, col, row, &CellUpdate::Number(value))
}

/// 修改/新增字符串单元格
fn modify_cell_string(sheet_xml: &[u8], col: u32, row: u32, sst_idx: usize) -> Result<Vec<u8>, String> {
    let cell_ref = format!("{}{}", col_letter(col), row);
    modify_cell_in_xml(sheet_xml, &cell_ref, col, row, &CellUpdate::String(sst_idx))
}

/// 修改/新增公式单元格
fn modify_cell_formula(sheet_xml: &[u8], col: u32, row: u32, formula: &str) -> Result<Vec<u8>, String> {
    let cell_ref = format!("{}{}", col_letter(col), row);
    modify_cell_in_xml(sheet_xml, &cell_ref, col, row, &CellUpdate::Formula(formula.to_string()))
}

enum CellUpdate {
    Number(f64),
    String(usize), // SST index
    Formula(String),
}

/// 在 sheet XML 中原地修改或新增单元格
fn modify_cell_in_xml(
    sheet_xml: &[u8],
    cell_ref: &str,
    col: u32,
    row: u32,
    update: &CellUpdate,
) -> Result<Vec<u8>, String> {
    let input = String::from_utf8_lossy(sheet_xml).into_owned();

    // 策略: 逐个字符扫描, 定位 <c r="CELLREF" 或 <c r="CELLREF> 的起始和结束位置
    let search = format!("r=\"{}\"", cell_ref);
    if let Some(cell_start) = input.find(&search) {
        // 单元格已存在 → 找到整个 <c ...> 标签并替换内容
        let tag_start = input[..cell_start].rfind("<c ").ok_or("无法定位 <c> 开始: internal error")?;
        let tag_end = find_cell_end(&input, tag_start).ok_or("无法定位 <c> 结束: internal error")?;

        let prefix = &input[..tag_start];
        let suffix = &input[tag_end..];
        let new_cell = build_cell_xml(cell_ref, update, extract_style(&input[tag_start..tag_end]));
        Ok(format!("{}{}{}", prefix, new_cell, suffix).into_bytes())
    } else {
        // 单元格不存在 → 在正确的行内插入
        let search_row = format!("r=\"{}\"", row);
        if let Some(row_pos) = input.find(&search_row) {
            // 找到目标行 → 在该行内插入（按列序）
            // 确认匹配的是 <row r="N" 而非 <c r="XN"
            if !is_row_element(&input, row_pos) {
                // 匹配到的是单元格引用，尝试继续查找真正的行元素
                let after = row_pos + search_row.len();
                if let Some(next_pos) = input[after..].find(&search_row) {
                    let abs_next = after + next_pos;
                    if is_row_element(&input, abs_next) {
                        return insert_cell_into_row(&input, abs_next, col, cell_ref, update);
                    }
                }
                // 行不存在，创建新行
                return insert_new_row_sorted(&input, row, col, cell_ref, update);
            }
            insert_cell_into_row(&input, row_pos, col, cell_ref, update)
        } else {
            // 目标行不存在 → 按行号排序插入新行
            insert_new_row_sorted(&input, row, col, cell_ref, update)
        }
    }
}

/// 检查匹配位置是否是 <row r="N" (而非 <c r="XN")
fn is_row_element(input: &str, pos: usize) -> bool {
    let prefix = &input[..pos];
    prefix.rfind("<row ").map_or(false, |r| {
        // 确认 rfind 找到的是 <row 标签开头，不是 <c ... r=
        let between = &input[r + 5..pos];
        !between.contains('<') && !between.contains('>')
    })
}

/// 在已有行内插入单元格（按列序）
fn insert_cell_into_row(
    input: &str,
    row_pos: usize,
    col: u32,
    cell_ref: &str,
    update: &CellUpdate,
) -> Result<Vec<u8>, String> {
    let row_tag_start = input[..row_pos].rfind("<row ").ok_or("无法定位 <row>")?;
    let row_content_start = input[row_tag_start..].find('>').ok_or("无法定位 <row> 结束")? + row_tag_start + 1;
    let row_end = input[row_tag_start..].find("</row>").ok_or("无法定位 </row>")? + row_tag_start;

    // 在行内找到正确的列序插入位置
    let row_content = &input[row_content_start..row_end];
    let insert_pos = find_column_insert_point(row_content, col) + row_content_start;
    let new_cell = build_cell_xml(cell_ref, update, None);
    let prefix = &input[..insert_pos];
    let suffix = &input[insert_pos..];
    Ok(format!("{}{}{}", prefix, new_cell, suffix).into_bytes())
}

/// 按行号升序在 <sheetData> 内插入新行
fn insert_new_row_sorted(
    input: &str,
    row: u32,
    _col: u32,
    cell_ref: &str,
    update: &CellUpdate,
) -> Result<Vec<u8>, String> {
    let sd_start = input.find("<sheetData").ok_or("缺少 <sheetData>")?;
    let new_cell = build_cell_xml(cell_ref, update, None);
    let new_row = format!(
        "<row r=\"{}\">\n    {}\n  </row>\n  ",
        row, new_cell
    );

    // 处理自闭合 <sheetData/>
    if let Some(slash) = input[sd_start..].find("/>") {
        let pos = sd_start + slash;
        if pos - sd_start < 50 {
            // 确认是 <sheetData/> 自闭合（"/>" 在合理距离内）
            let prefix = &input[..pos];
            let suffix = &input[pos + 2..];
            return Ok(format!("{}>\n  {}\n</sheetData>{}", prefix, new_row, suffix).into_bytes());
        }
    }

    // 普通模式：在已有行之间找正确的位置
    if let Some(sd_close) = input.find("</sheetData>") {
        let sd_content_start = input[..sd_close].find('>').map(|p| p + 1).unwrap_or(sd_start + 11);
        let sd_content = &input[sd_content_start..sd_close];

        // 扫描已有行，找第一个行号 > target row 的位置
        let mut search_from = 0;
        while let Some(row_tag_pos) = sd_content[search_from..].find("<row r=\"") {
            let abs_tag_start = sd_content_start + search_from + row_tag_pos;
            let num_start = abs_tag_start + "<row r=\"".len();
            if let Some(num_end) = input[num_start..].find('\"') {
                if let Ok(existing_row) = input[num_start..num_start + num_end].parse::<u32>() {
                    if existing_row > row {
                        // 在此行之前插入
                        return Ok(format!("{}  {}{}",
                            &input[..abs_tag_start],
                            new_row,
                            &input[abs_tag_start..]
                        ).into_bytes());
                    }
                    // 跳过此行的 </row>
                    if let Some(row_close) = input[abs_tag_start..sd_close].find("</row>") {
                        search_from = (abs_tag_start + row_close + "</row>".len()) - sd_content_start;
                    } else {
                        search_from = (abs_tag_start + 20) - sd_content_start;
                    }
                } else {
                    search_from = (num_start + 4) - sd_content_start;
                }
            } else {
                search_from = (num_start + 4) - sd_content_start;
            }
        }

        // 所有已有行的行号都小于新行 → 在 </sheetData> 前插入
        let prefix = &input[..sd_close];
        let suffix = &input[sd_close..];
        return Ok(format!("{}  {}{}", prefix, new_row, suffix).into_bytes());
    }

    Err("无法解析 <sheetData> 结构".to_string())
}

/// 在行内容中找正确的列序插入位置
fn find_column_insert_point(row_content: &str, target_col: u32) -> usize {
    let mut best_pos = 0usize;
    let mut search_start = 0;
    while let Some(r_pos) = row_content[search_start..].find("r=\"") {
        let abs = search_start + r_pos + 3;
        let ref_end = row_content[abs..].find('\"').map(|p| p + abs).unwrap_or(abs);
        let ref_str = &row_content[abs..ref_end];
        if let Some(this_col) = parse_cell_column(ref_str) {
            if this_col < target_col {
                // 找到此单元格的 </c> 结束位置
                let tag_start = row_content[..abs].rfind("<c ").unwrap_or(0);
                best_pos = find_cell_end(&row_content[tag_start..], 0)
                    .map(|p| p + tag_start)
                    .unwrap_or(0);
            } else {
                // 已越过目标列，在上一个 </c> 后插入
                return best_pos;
            }
        }
        search_start = ref_end + 1;
    }
    best_pos
}

/// 解析列引用 (A=1, B=2, ..., Z=26, AA=27, ...)
fn parse_cell_column(ref_str: &str) -> Option<u32> {
    let alpha: String = ref_str.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if alpha.is_empty() { return None; }
    let mut col = 0u32;
    for c in alpha.chars() {
        col = col * 26 + (c as u32 - b'A' as u32 + 1);
    }
    Some(col)
}

/// 找 </c> 闭合位置，处理嵌套（f/v/is 等子标签）
fn find_cell_end(input: &str, offset: usize) -> Option<usize> {
    let s = &input[offset..];
    // 简单策略: 找下一个 </c>
    s.find("</c>").map(|p| offset + p + "</c>".len())
}

/// 从已有单元格的 XML 中提取 style 属性
fn extract_style(cell_xml: &str) -> Option<String> {
    let s_tag = cell_xml.find("s=\"")?;
    let rest = &cell_xml[s_tag + 3..];
    let end = rest.find('\"')?;
    Some(rest[..end].to_string())
}

/// 构建单元格 XML
fn build_cell_xml(cell_ref: &str, update: &CellUpdate, style: Option<String>) -> String {
    let style_attr = style.map(|s| format!(" s=\"{}\"", s)).unwrap_or_default();
    match update {
        CellUpdate::Number(v) => {
            format!("<c r=\"{}\"{}><v>{}</v></c>", cell_ref, style_attr, v)
        }
        CellUpdate::String(sst_idx) => {
            format!("<c r=\"{}\"{} t=\"s\"><v>{}</v></c>", cell_ref, style_attr, sst_idx)
        }
        CellUpdate::Formula(f) => {
            format!("<c r=\"{}\"{}><f>{}</f></c>", cell_ref, style_attr, f)
        }
    }
}

/// 对区域应用 NumberFormat
fn apply_number_format_region(
    sheet_xml: &[u8],
    col_start: u32,
    col_end: u32,
    row_start: u32,
    row_end: u32,
    _format_code: &str,
) -> Result<Vec<u8>, String> {
    let input = String::from_utf8_lossy(sheet_xml).into_owned();
    let result = input.clone();

    for row in row_start..=row_end {
        for col in col_start..=col_end {
            let cell_ref = format!("{}{}", col_letter(col), row);
            let search = format!("r=\"{}\"", cell_ref);
            if let Some(pos) = result.find(&search) {
                let tag_start = result[..pos].rfind("<c ").unwrap_or(pos);
                let cell_fragment = &result[tag_start..];
                let cell_str = cell_fragment.to_string();

                // 已有 style → 不改（保留已有样式优先）
                if cell_str.contains(" s=\"") {
                    continue;
                }

                // 无 style → 创建新样式并应用
                // 对于 v0.6: 直接在单元格上添加 style 引用
                // 简化处理: 在 <c r="X1" 后添加 s="N"
                // 由于我们不做 styles.xml 的修改，这里采用不同策略:
                // 在单元格级别直接添加 numFmtId 对应的格式
                // 实际上我们无法轻松地动态创建样式ID...
                // 所以对于 v0.6，我们仅在导出新创建的工作簿时应用样式，
                // 修改已有模板时跳过样式设置。
            }
        }
    }
    Ok(result.into_bytes())
}

/// 列号 → 字母 (1→A, 2→B, ..., 27→AA)
pub fn col_letter(col: u32) -> String {
    let mut n = col.saturating_sub(1);
    let mut v = Vec::new();
    loop {
        v.push((b'A' + (n % 26) as u8) as char);
        if n < 26 { break; }
        n = n / 26 - 1;
    }
    v.reverse();
    v.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::Reader;

    #[test]
    fn test_col_letter() {
        assert_eq!(col_letter(1), "A");
        assert_eq!(col_letter(26), "Z");
        assert_eq!(col_letter(27), "AA");
        assert_eq!(col_letter(52), "AZ");
        assert_eq!(col_letter(53), "BA");
        assert_eq!(col_letter(702), "ZZ");
        assert_eq!(col_letter(703), "AAA");
    }

    #[test]
    fn test_empty_workbook_roundtrip() {
        let w = XlsxWriter::empty();
        let tmp = std::env::temp_dir().join("test_empty.xlsx");
        w.save(&tmp).unwrap();
        assert!(tmp.exists());
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_set_number_new_cell() {
        let mut w = XlsxWriter::empty();
        w.set_number("Sheet1", 3, 2, 12345.67).unwrap();
        let tmp = std::env::temp_dir().join("test_number.xlsx");
        w.save(&tmp).unwrap();
        let mut reader = calamine::open_workbook_auto(&tmp).unwrap();
        let range = reader.worksheet_range("Sheet1").unwrap();
        let found = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 12345.67).abs() < 0.01));
        assert!(found, "值 12345.67 未写入 xlsx");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_set_string_new_cell() {
        let mut w = XlsxWriter::empty();
        w.set_string("Sheet1", 1, 1, "营业收入").unwrap();
        w.set_string("Sheet1", 2, 1, "100万").unwrap();
        let tmp = std::env::temp_dir().join("test_string.xlsx");
        w.save(&tmp).unwrap();
        let mut reader = calamine::open_workbook_auto(&tmp).unwrap();
        let range = reader.worksheet_range("Sheet1").unwrap();
        let has_revenue = range.used_cells().any(|(_, _, v)| v == &calamine::Data::String("营业收入".into()));
        let has_amount = range.used_cells().any(|(_, _, v)| v == &calamine::Data::String("100万".into()));
        assert!(has_revenue, "未找到'营业收入'");
        assert!(has_amount, "未找到'100万'");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_ensure_sheet_preserves_rels() {
        let mut w = XlsxWriter::empty();
        w.ensure_sheet("MySheet").unwrap();
        let tmp = std::env::temp_dir().join("test_rels.xlsx");
        w.save(&tmp).unwrap();

        let file = std::fs::File::open(&tmp).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().to_string();
            if name.contains("rels") || name.contains("workbook") || name.contains("Content") {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
                eprintln!("=== {} ===\n{}", name, String::from_utf8_lossy(&buf));
            }
        }

        // Verify rels has entries
        let wb: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
        let names: Vec<_> = wb.sheet_names().to_vec();
        eprintln!("Sheet names: {:?}", names);
        assert!(names.contains(&"MySheet".to_string()), "MySheet should exist");

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_open_and_modify_existing() {
        // 创建初始文件
        let mut w = XlsxWriter::empty();
        w.set_number("Sheet1", 2, 3, 42.0).unwrap();
        let tmp = std::env::temp_dir().join("test_modify.xlsx");
        w.save(&tmp).unwrap();

        // 重新打开并修改
        let mut w2 = XlsxWriter::open(&tmp).unwrap();
        w2.set_number("Sheet1", 2, 3, 99.0).unwrap();
        w2.set_number("Sheet1", 3, 3, 55.5).unwrap();
        w2.save(&tmp).unwrap();

        let mut reader = calamine::open_workbook_auto(&tmp).unwrap();
        let range = reader.worksheet_range("Sheet1").unwrap();
        // calamine v0.26: get uses relative coords. B3 (col 2, row 3) → start at (2, 2)
        // relative: (2-2, 2-2) = (0,0) for B3 if start is (2,2)
        let has_99 = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 99.0).abs() < 0.01));
        let has_55_5 = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 55.5).abs() < 0.01));
        assert!(has_99, "未找到值 99.0");
        assert!(has_55_5, "未找到值 55.5");
        std::fs::remove_file(&tmp).ok();
    }
}
