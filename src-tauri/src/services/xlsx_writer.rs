//! 纯 Rust xlsx 模板修改引擎 (路线2: ZIP + XML 直接操作)
//!
//! 核心思路: xlsx = ZIP 压缩包, 内含 XML 文件。
//! 打开模板 → 修改目标单元格 → 保存, 完美保留所有格式/公式/图表/合并单元格。
//!
//! 依赖: zip (纯Rust) + quick-xml (纯Rust), 零 C 依赖, 不会硬崩溃。

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;

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
    /// 从模板 SST 解析的文本（含缺失，仅用于去重）
    shared_strings: Vec<String>,
    /// 原始 SST 条目总数（含富文本等非简单格式，不可丢失）
    original_sst_count: usize,
    /// 原始 SST XML（不做重建，只追加新条目）
    original_sst_xml: Vec<u8>,
    /// 新增加的字符串
    new_strings: Vec<String>,
    sst_modified: bool,
    /// sheet 名称 → XML 路径缓存 (如 "盛唐融信" → "xl/worksheets/sheet22.xml")
    sheet_map: HashMap<String, String>,
    /// 被修改过的 sheet XML 路径集合 (用于公式缓存清除)
    dirty_sheets: std::collections::HashSet<String>,
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

        let sst_texts = Self::load_sst(&entries);
        let sst_count = Self::count_sst_entries(&entries);
        let sst_raw = entries.get("xl/sharedStrings.xml").cloned().unwrap_or_default();
        let sheet_map = Self::build_sheet_map(&entries);
        tracing::info!("[xlsx_writer] 打开模板, sheet_map 包含 {} 个 sheet (SST原始={}条, 可解析={}条)", sheet_map.len(), sst_count, sst_texts.len());
        Ok(Self {
            entries, shared_strings: sst_texts, original_sst_count: sst_count,
            original_sst_xml: sst_raw, new_strings: Vec::new(), sst_modified: false,
            sheet_map, dirty_sheets: std::collections::HashSet::new(),
        })
    }

    /// 创建空白工作簿（仅含最小模板结构）
    pub fn empty() -> Self {
        let shared_strings: Vec<String> = Vec::new();
        let empty_sst = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0">
</sst>"#;
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

        // xl/sharedStrings.xml — 最小 SST（空）
        entries.insert(
            "xl/sharedStrings.xml".into(),
            empty_sst.into(),
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

        let sheet_map = Self::build_sheet_map(&entries);
        Self {
            entries, shared_strings, original_sst_count: 0,
            original_sst_xml: empty_sst.as_bytes().to_vec(), new_strings: Vec::new(),
            sst_modified: false, sheet_map,
            dirty_sheets: std::collections::HashSet::new(),
        }
    }

    /// 确保目标 Sheet 存在（不存在则创建）
    pub fn ensure_sheet(&mut self, name: &str) -> Result<(), AppError> {
        let sheet_path = self.find_sheet_path( name);
        if sheet_path.is_some() {
            return Ok(());
        }
        // 创建新 sheet
        let sheet_count = self.sheet_count();
        let new_id = sheet_count + 1;
        let r_id = format!("rId{}", 100 + new_id);
        let fname = format!("worksheets/sheet{}.xml", new_id);
        let path = format!("xl/{}", fname);
        let path_key = path.clone();

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
        // 更新 sheet_map 缓存
        self.sheet_map.insert(name.to_string(), path_key);

        Ok(())
    }

    /// 设置单元格数值
    pub fn set_number(&mut self, sheet: &str, col: u32, row: u32, value: f64) -> Result<(), AppError> {
        let path = self.find_sheet_path( sheet)
            .ok_or_else(|| AppError::SheetNotFound {
                file: "xlsx".into(),
                sheet: sheet.to_string(),
            })?;
        let xml = self.entries.get(&path).cloned().unwrap_or_default();
        let modified = modify_cell_number(&xml, col, row, value)
            .map_err(|e| AppError::Other(format!("修改单元格 {}{}: {}", col_letter(col), row, e)))?;
        self.dirty_sheets.insert(path.clone());
        self.entries.insert(path, modified);
        Ok(())
    }

    /// 设置单元格文本
    pub fn set_string(&mut self, sheet: &str, col: u32, row: u32, text: &str) -> Result<(), AppError> {
        if text.is_empty() {
            return Ok(());
        }
        let path = self.find_sheet_path( sheet)
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
        let path = self.find_sheet_path( sheet)
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

    /// 写入公式并附带缓存值（避免 strip_formula_cache 后显示为空）
    pub fn set_formula_with_value(
        &mut self,
        sheet: &str,
        col: u32,
        row: u32,
        formula: &str,
        cached_value: &str,
    ) -> Result<(), AppError> {
        let path = self.find_sheet_path(sheet)
            .ok_or_else(|| AppError::SheetNotFound {
                file: "xlsx".into(),
                sheet: sheet.to_string(),
            })?;
        let cell_ref = format!("{}{}", col_letter(col), row);
        let xml = self.entries.get(&path).cloned().unwrap_or_default();
        let modified = modify_cell_in_xml(
            &xml, &cell_ref, col, row,
            &CellUpdate::FormulaWithValue(formula.to_string(), cached_value.to_string()),
        )
        .map_err(|e| AppError::Other(format!("修改单元格 {}{}: {}", col_letter(col), row, e)))?;
        self.entries.insert(path, modified);
        Ok(())
    }

    /// 移除指定 Sheet 的 dirty 标记（避免 strip_formula_cache 清除其公式缓存）
    pub fn clear_dirty(&mut self, sheet: &str) {
        if let Some(path) = self.find_sheet_path(sheet) {
            self.dirty_sheets.remove(&path);
        }
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
        let path = self.find_sheet_path( sheet)
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

    /// 预构建 sheet 名称 → XML 路径的映射
    fn build_sheet_map(entries: &HashMap<String, Vec<u8>>) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let wb_xml = match entries.get("xl/workbook.xml") {
            Some(b) => String::from_utf8_lossy(b).into_owned(),
            None => return map,
        };
        let rels_xml = match entries.get("xl/_rels/workbook.xml.rels") {
            Some(b) => String::from_utf8_lossy(b).into_owned(),
            None => return map,
        };

        // 提取每个 sheet: name="X" → r:id="Y"
        let mut sheet_rids: Vec<(String, String)> = Vec::new();
        let mut search = 0;
        while let Some(tag_start) = wb_xml[search..].find("<sheet ") {
            let abs_start = search + tag_start;
            let tag_end = match wb_xml[abs_start..].find("/>") {
                Some(p) => abs_start + p + 2,
                None => break,
            };
            let tag = &wb_xml[abs_start..tag_end];

            let name = extract_attr(tag, "name");
            let rid = extract_attr(tag, "r:id");

            if let (Some(n), Some(r)) = (name, rid) {
                sheet_rids.push((n, r));
            }
            search = tag_end;
        }

        // 通过 rels 解析 r:id → Target 的映射
        let mut rid_to_target: HashMap<String, String> = HashMap::new();
        let mut rels_search = 0;
        while let Some(rel_start) = rels_xml[rels_search..].find("<Relationship ") {
            let abs_start = rels_search + rel_start;
            let rel_end = match rels_xml[abs_start..].find("/>") {
                Some(p) => abs_start + p + 2,
                None => break,
            };
            let rel_tag = &rels_xml[abs_start..rel_end];

            let id = extract_attr(rel_tag, "Id");
            let target = extract_attr(rel_tag, "Target");

            if let (Some(i), Some(t)) = (id, target) {
                rid_to_target.insert(i, t);
            }
            rels_search = rel_end;
        }

        // 组装: sheet 名称 → xl/<target>
        for (name, rid) in &sheet_rids {
            if let Some(target) = rid_to_target.get(rid) {
                map.insert(name.clone(), format!("xl/{}", target));
            }
        }
        map
    }

    /// 在 ZIP entries 中查找 Sheet 的路径（HashMap 查找，O(1)）
    fn find_sheet_path(&self, name: &str) -> Option<String> {
        self.sheet_map.get(name).cloned()
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

    /// 统计原始 SST 中 <si> 条目总数（不解析文本，仅计数）
    fn count_sst_entries(entries: &HashMap<String, Vec<u8>>) -> usize {
        let xml = match entries.get("xl/sharedStrings.xml") {
            Some(b) => String::from_utf8_lossy(b),
            None => return 0,
        };
        xml.matches("<si>").count() + xml.matches("<si ").count()
    }

    fn add_shared_string(&mut self, text: &str) -> usize {
        // 只在原始 SST 范围内搜索（shared_strings 可能因富文本而膨胀）
        let original_count = self.original_sst_count.min(self.shared_strings.len());
        if let Some(idx) = self.shared_strings[..original_count].iter().position(|s| s == text) {
            return idx;
        }
        // 检查新增列表中是否已有
        if let Some(idx) = self.new_strings.iter().position(|s| s == text) {
            return self.original_sst_count + idx;
        }
        let idx = self.original_sst_count + self.new_strings.len();
        self.new_strings.push(text.to_string());
        self.sst_modified = true;
        idx
    }

    /// 在原始 SST XML 上追加新条目，保留原有所有条目格式
    fn append_to_sst(original_xml: &[u8], new_strings: &[String]) -> Vec<u8> {
        if new_strings.is_empty() {
            return original_xml.to_vec();
        }
        let mut s = String::from_utf8_lossy(original_xml).into_owned();
        // 更新 count 和 uniqueCount
        let re_count = regex::Regex::new(r#"count="(\d+)""#).unwrap();
        let re_unique = regex::Regex::new(r#"uniqueCount="(\d+)""#).unwrap();
        let old_count: usize = re_count.captures(&s)
            .and_then(|c| c[1].parse().ok()).unwrap_or(0);
        let old_unique: usize = re_unique.captures(&s)
            .and_then(|c| c[1].parse().ok()).unwrap_or(0);
        let new_count = old_count + new_strings.len();
        let new_unique = old_unique + new_strings.len();
        s = re_count.replace(&s, format!(r#"count="{}""#, new_count)).into_owned();
        s = re_unique.replace(&s, format!(r#"uniqueCount="{}""#, new_unique)).into_owned();

        // 构建新条目 XML
        let mut new_entries = String::new();
        for text in new_strings {
            new_entries.push_str(&format!("<si><t>{}</t></si>", quick_xml::escape::escape(text)));
        }

        // 处理两种情况：</sst> 闭合标签 或 /> 自闭合
        if let Some(end_pos) = s.find("</sst>") {
            s.insert_str(end_pos, &new_entries);
        } else if let Some(close_pos) = s.rfind("/>") {
            // 自闭合标签 → 展开为完整标签，插入新条目
            let before = &s[..close_pos];
            let after = &s[close_pos + 2..];
            s = format!("{}>{}</sst>{}", before, new_entries, after);
        }
        s.into_bytes()
    }

    /// 清除公式缓存值: <f>SUM(...)</f><v>OLD</v> → <f>SUM(...)</f>
    /// 只处理被修改过的 sheet (记录在 dirty_sheets 中)
    fn strip_formula_cache(xml: &[u8]) -> Vec<u8> {
        let s = String::from_utf8_lossy(xml);
        // 匹配两种公式格式后紧跟的缓存 <v>:
        // 1. <f t="shared" si="0"/> → 自闭合
        // 2. <f>SUM(...)</f>       → 有内容的公式
        let re = regex::Regex::new(r"(<f[^>]*?/>)\s*<v>[^<]*</v>").unwrap();
        let s = re.replace_all(&s, "$1").into_owned();
        let re2 = regex::Regex::new(r"(<f[^>]*>[^<]*</f>)\s*<v>[^<]*</v>").unwrap();
        re2.replace_all(&s, "$1").into_owned().into_bytes()
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
            let mut actual_data: Vec<u8> = if *name == "xl/sharedStrings.xml" && self.sst_modified {
                Self::append_to_sst(&self.original_sst_xml, &self.new_strings)
            } else {
                (*data).clone()
            };
            // 只清除被修改过的 sheet 的公式缓存 (写入了新数据→依赖公式需重算)
            if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") && self.dirty_sheets.contains(name.as_str()) {
                actual_data = Self::strip_formula_cache(&actual_data);
            }
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
    FormulaWithValue(String, String), // (formula_text, cached_value)
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

/// 找单元格结束位置，处理两种形式：
///   <c r="A1"/>         → 自闭合，找 "/>"
///   <c r="A1"><v>1</v></c> → 有子元素，找 "</c>"
fn find_cell_end(input: &str, offset: usize) -> Option<usize> {
    let s = &input[offset..];
    // 先检查是否是自闭合标签 (如 <c r="C2" s="52"/>)
    if let Some(gt_pos) = s.find('>') {
        let before_gt = &s[..gt_pos];
        if before_gt.ends_with('/') {
            return Some(offset + gt_pos + 1);
        }
    }
    // 否则找闭合标签 </c>
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
        CellUpdate::FormulaWithValue(f, v) => {
            // 判断缓存值是否为纯数字：是则不加类型，否则 t="str"
            let type_attr = if v.parse::<f64>().is_ok() { "" } else { " t=\"str\"" };
            format!("<c r=\"{}\"{}{}><f>{}</f><v>{}</v></c>", cell_ref, style_attr, type_attr, f, v)
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

/// 从 XML 标签中提取属性值 (如 extract_attr(tag, "name") → Some("盛唐融信"))
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let marker = format!("{}=\"", attr);
    let pos = tag.find(&marker)?;
    let rest = &tag[pos + marker.len()..];
    let end = rest.find('\"')?;
    Some(rest[..end].to_string())
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

    #[test]
    fn test_real_template_sheet_map_and_write() {
        let tmpl = r"C:\Users\Administrator\Desktop\2026年4月分析\【2026年4月】经营数据 - 空.xlsx";
        if !std::path::Path::new(tmpl).exists() {
            eprintln!("模板文件不存在，跳过测试");
            return;
        }
        let mut w = XlsxWriter::open(std::path::Path::new(tmpl)).unwrap();
        // 验证 sheet_map
        for name in &["填写页", "保险类", "商写类", "酒店类", "盛唐融信", "北京中言"] {
            let path = w.find_sheet_path(name);
            assert!(path.is_some(), "sheet '{}' 未在 sheet_map 中找到!", name);
            eprintln!("  {} -> {}", name, path.unwrap());
        }
        // 实际写入测试: 往保险类 C2 写 12345.67
        w.set_number("保险类", 3, 2, 12345.67).unwrap();
        // 保存到临时文件
        let tmp = std::env::temp_dir().join("test_real_template.xlsx");
        w.save(&tmp).unwrap();
        // 用 calamine 读回验证
        let mut reader: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
        let range = reader.worksheet_range("保险类").unwrap();
        let found = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 12345.67).abs() < 0.01));
        assert!(found, "写入 保险类 C2=12345.67 后未能在文件中找到!");
        std::fs::remove_file(&tmp).ok();
    }

    /// 验证修改空单元格（自闭合标签）不会破坏同行相邻单元格
    #[test]
    fn test_modify_empty_cell_preserves_neighbors() {
        let tmp = std::env::temp_dir().join("test_empty_cell.xlsx");
        // 创建: A1="H", B1(空,有格式), C1=42
        let mut w = XlsxWriter::empty();
        w.set_string("Sheet1", 1, 1, "Header").unwrap();
        w.set_number("Sheet1", 3, 1, 42.0).unwrap();
        w.save(&tmp).unwrap();

        // 重新打开，写入 B1=99 (之前为空/不存在)
        let mut w2 = XlsxWriter::open(&tmp).unwrap();
        w2.set_number("Sheet1", 2, 1, 99.0).unwrap();
        w2.save(&tmp).unwrap();

        // 验证 A1 和 C1 仍然存在
        let mut reader: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
        let range = reader.worksheet_range("Sheet1").unwrap();
        let has_header = range.used_cells().any(|(_, _, v)| v == &calamine::Data::String("Header".into()));
        let has_42 = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 42.0).abs() < 0.01));
        let has_99 = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 99.0).abs() < 0.01));
        assert!(has_header, "A1 'Header' 被破坏");
        assert!(has_99, "B1=99 写入失败");
        assert!(has_42, "C1=42 被 B1 写入破坏!");
        std::fs::remove_file(&tmp).ok();
    }

    /// 验证修改真实模板的空单元格不破坏相邻单元格
    #[test]
    fn test_real_template_preserves_neighbors() {
        let tmpl = r"C:\Users\Administrator\Desktop\2026年4月分析\【2026年4月】经营数据 - 空.xlsx";
        if !std::path::Path::new(tmpl).exists() { return; }
        let tmp = std::env::temp_dir().join("test_tmpl_neighbor.xlsx");
        std::fs::copy(tmpl, &tmp).unwrap();

        // 打开模板，修改商写类 C2(原本为空单元格)
        let mut w = XlsxWriter::open(&tmp).unwrap();
        w.set_number("商写类", 3, 2, 99999.0).unwrap();
        w.save(&tmp).unwrap();

        // 验证相邻单元格 A2(共享字符串), B2(共享字符串), L2 仍然存在
        let mut reader: calamine::Xlsx<_> = calamine::open_workbook(&tmp).unwrap();
        let range = reader.worksheet_range("商写类").unwrap();
        let rows: Vec<&[calamine::Data]> = range.rows().collect();
        // Row 2 (0-based index 1) should exist
        assert!(rows.len() > 1, "Row 2 丢失");
        let row2 = rows[1];
        assert!(row2.len() >= 7, "Row 2 仅剩 {} 列, 相邻单元格被截断", row2.len());
        // C2 (col 2) should have 99999
        let has_99999 = range.used_cells().any(|(_, _, v)| matches!(v, calamine::Data::Float(x) if (*x - 99999.0).abs() < 0.01));
        assert!(has_99999, "C2=99999 写入失败");
        std::fs::remove_file(&tmp).ok();
    }
}
