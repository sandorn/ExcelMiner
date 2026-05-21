//! 最小 XLSX 读写验证
use std::io::Write;
use excelminer_lib::services::excel_reader::ExcelReader;

#[test]
fn test_xlsx_roundtrip() {
    // 手动创建包含数字的 xlsx
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.xlsx");

    // 手动创建 sheet XML: 在 D4 写入 100
    let mut sheet_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#);
    // Row 4: D4=100
    sheet_xml.push_str(r#"<row r="4"><c r="D4"><v>100</v></c></row>"#);
    sheet_xml.push_str("</sheetData></worksheet>");

    let f = std::fs::File::create(&path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    z.start_file("[Content_Types].xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#).unwrap();

    z.start_file("_rels/.rels", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).unwrap();

    z.start_file("xl/sharedStrings.xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t> </t></si></sst>"#).unwrap();

    z.start_file("xl/workbook.xml", o).unwrap();
    z.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheets><sheet name=\"Test\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>").unwrap();

    z.start_file("xl/_rels/workbook.xml.rels", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();

    z.start_file("xl/worksheets/sheet1.xml", o).unwrap();
    z.write_all(sheet_xml.as_bytes()).unwrap();
    z.finish().unwrap();

    // 读取
    let mut reader = ExcelReader::open(&path).unwrap();
    let data = reader.read_sheet("Test").unwrap();

    eprintln!("Headers: {:?}", &data.headers);
    eprintln!("Row count: {}", data.rows.len());
    for (i, row) in data.rows.iter().enumerate() {
        eprintln!("Row {}: {:?}", i + 1, row);
    }

    // rows 是 0-based: rows[3] = VBA row 4
    if data.rows.len() >= 4 {
        let row4 = &data.rows[3];
        eprintln!("Row4 (0-based idx 3): {:?}, len={}", row4, row4.len());
        if row4.len() >= 4 {
            eprintln!("Row4[3] = '{}'", row4[3]);
        } else {
            eprintln!("Row4 too short: {} < 4", row4.len());
        }
    } else {
        eprintln!("Only {} rows, expected at least 4", data.rows.len());
    }
}
