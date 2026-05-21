//! 汇总引擎集成测试

use std::path::Path;
use std::io::Write;

use excelminer_lib::models::project::{Project, Company, BusinessType};
use excelminer_lib::services::data_aggregator::insurance::InsuranceAggregator;
use excelminer_lib::services::data_aggregator::commercial::CommercialAggregator;
use excelminer_lib::services::data_aggregator::hotel::HotelAggregator;
use excelminer_lib::services::data_aggregator::financial::FinancialAggregator;
use excelminer_lib::services::data_aggregator::AggregationEngine;

fn create_xlsx(path: &Path, sheet_name: &str, rows: &[Vec<String>]) {
    let mut sheet_xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1"/><sheetData>"#
    );
    for (ri, row) in rows.iter().enumerate() {
        sheet_xml.push_str(&format!(r#"<row r="{}">"#, ri + 1));
        // 始终写入 A 列和最后一个有值的列，确保 calamine 正确定位列范围
        let max_col = row.iter().rposition(|c| !c.is_empty()).unwrap_or(0);
        for (ci, cell) in row.iter().enumerate() {
            if ci > 0 && cell.is_empty() && ci < max_col { continue; } // 跳过中间空单元格
            let col = (b'A' + ci as u8) as char;
            let rf = format!("{}{}", col, ri + 1);
            if let Ok(v) = cell.parse::<f64>() {
                sheet_xml.push_str(&format!(r#"<c r="{}"><v>{}</v></c>"#, rf, v));
            } else if cell.is_empty() {
                // 空单元格也写入数字0以锚定列
                sheet_xml.push_str(&format!(r#"<c r="{}"><v>0</v></c>"#, rf));
            } else {
                sheet_xml.push_str(&format!(r#"<c r="{}" t="s"><v>0</v></c>"#, rf));
            }
        }
        sheet_xml.push_str("</row>");
    }
    sheet_xml.push_str("</sheetData></worksheet>");

    let f = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    z.start_file("[Content_Types].xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#).unwrap();

    z.start_file("_rels/.rels", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).unwrap();

    // 共享字符串表: 只放一个空串
    z.start_file("xl/sharedStrings.xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t> </t></si></sst>"#).unwrap();

    let wb = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets></workbook>"#, sheet_name);
    z.start_file("xl/workbook.xml", o).unwrap(); z.write_all(wb.as_bytes()).unwrap();

    z.start_file("xl/_rels/workbook.xml.rels", o).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();

    z.start_file("xl/worksheets/sheet1.xml", o).unwrap();
    z.write_all(sheet_xml.as_bytes()).unwrap();
    z.finish().unwrap();
}

fn tmpdir() -> tempfile::TempDir { tempfile::TempDir::new().unwrap() }
fn empty_row(n: usize) -> Vec<String> { vec![String::new(); n] }
fn mkproj(tmp: &Path) -> Project { Project { name:"2024年6月".into(), year:2024, month:6, data_folder:tmp.to_path_buf(), output_file:tmp.join("o.xlsx"), companies:vec![], ytd_months:6, ai_config:Default::default() } }

// ========== 测试 ==========

#[test]
fn test_insurance() {
    let t = tmpdir(); let a = t.path().join("活动量"); std::fs::create_dir_all(&a).unwrap();
    let mut r = vec![empty_row(15); 18];
    r[3][3]="100".into(); // Row4 D4
    for m in 0..6 { r[4][2*m+3]=format!("{}",(m+1)*10+10); } //入职
    for (i,&v) in [5,10,10,5,5,5_i64].iter().enumerate() { r[5][2*i+3]=format!("{}",v); } 
    for m in 0..6 { r[9][2*m+3]="20".into(); r[11][2*m+3]="5000".into(); r[12][2*m+3]="3000".into(); r[17][2*m+3]="10".into(); }
    let lc=2*6+2; for ri in 13..=16 { r[ri][lc]="1000".into(); }
    create_xlsx(&a.join("盛唐融信.xlsx"),"保险类",&r); create_xlsx(&a.join("君康经纪.xlsx"),"保险类",&r);

    let res = InsuranceAggregator.execute(&mkproj(t.path())).unwrap();
    assert_eq!(res.companies_processed,2); assert!(res.warnings.is_empty());
    let cs:Vec<serde_json::Value>=serde_json::from_str(&res.summary_data).unwrap();
    assert_eq!(cs[0]["人力"]["期初人力"],serde_json::json!(100.0));
    assert_eq!(cs[0]["人力"]["YTD入职"],serde_json::json!(270.0));
    assert_eq!(cs[0]["人力"]["YTD离职"],serde_json::json!(40.0));
    assert_eq!(cs[0]["保费"]["新单规模保费YTD"],serde_json::json!(30000.0));
}

#[test]
fn test_commercial() {
    let t = tmpdir(); let a = t.path().join("活动量"); std::fs::create_dir_all(&a).unwrap();
    let mut r = vec![empty_row(15); 20];
    r[3][3]="5000".into();
    for m in 0..6 { r[4][2*m+3]="200".into(); r[6][2*m+3]="50".into(); r[8][2*m+3]="100".into(); r[9][2*m+3]="20".into(); r[10][2*m+3]="150".into(); r[13][2*m+3]="80".into(); r[14][2*m+3]="15".into(); r[15][2*m+3]="100".into(); r[18][2*m+3]="80".into(); r[19][2*m+3]="60".into(); }
    r[7][2*6+2]="5300".into();
    for n in &["北京中言","大连凯丹","福建钱隆","春夏秋冬","重庆宜新"] { create_xlsx(&a.join(format!("{}.xlsx",n)),"写字楼和商业综合体类",&r); }
    let res = CommercialAggregator.execute(&mkproj(t.path())).unwrap();
    assert_eq!(res.companies_processed,5);
    let cs:Vec<serde_json::Value>=serde_json::from_str(&res.summary_data).unwrap();
    assert!((cs[0]["渠道"]["转化率"].as_f64().unwrap()-0.2).abs()<0.01);
}

#[test]
fn test_hotel() {
    let t = tmpdir(); let a = t.path().join("活动量"); let rp=t.path().join("经营报表");
    std::fs::create_dir_all(&a).unwrap(); std::fs::create_dir_all(&rp).unwrap();

    // BHRT: 达成列 E=5(col4), G=7(col6), I=9(col8)... (0-based: 4,6,8,10,12,14)
    let mut bh: Vec<Vec<String>> = (0..21).map(|_| vec!["0".to_string(); 20]).collect();
    for &(row,val) in &[(12,100),(13,100),(14,100),(15,500),(16,500),(17,500),(18,50),(19,50),(20,50)] {
        for m in 0..6 { bh[row-1][2*m+4]=val.to_string(); }
    }
    create_xlsx(&a.join("伯豪瑞廷.xlsx"),"酒店类",&bh);
    // CQRER: 达成列 D=4(col3), F=6(col5), H=8(col7)... (0-based: 3,5,7,9,11,13)
    let mut cq: Vec<Vec<String>> = (0..15).map(|_| vec!["0".to_string(); 20]).collect();
    for &(row,val) in &[(12,"100"),(13,"500"),(14,"50")] {
        for m in 0..6 { cq[row-1][2*m+3]=val.to_string(); }
    }
    create_xlsx(&a.join("重庆瑞尔.xlsx"),"酒店类",&cq);

    let mut rep = vec![empty_row(18); 20];
    for ri in 3..20 { rep[ri]=empty_row(18); rep[ri][0]=format!("指标{}",ri-2); for ci in 3..15 { rep[ri][ci]="100".into(); } }
    create_xlsx(&rp.join("伯豪瑞廷.xlsx"),"指标统计",&rep);
    create_xlsx(&rp.join("重庆瑞尔.xlsx"),"指标统计",&rep);

    let res = HotelAggregator.execute(&mkproj(t.path())).unwrap();
    assert_eq!(res.companies_processed,2);
    let cs:Vec<serde_json::Value>=serde_json::from_str(&res.summary_data).unwrap();
    assert_eq!(cs[0]["营销活动"]["投放数量"],serde_json::json!(1800.0));
    assert_eq!(cs[1]["营销活动"]["投放数量"],serde_json::json!(600.0));
}

#[test]
fn test_financial() {
    let t = tmpdir(); let rp=t.path().join("经营报表"); std::fs::create_dir_all(&rp).unwrap();
    let mut rep = vec![empty_row(18); 20];
    for ri in 3..20 { rep[ri]=empty_row(18); rep[ri][0]=format!("指标{}",ri-2); for ci in 3..15 { rep[ri][ci]="100".into(); } }
    create_xlsx(&rp.join("A.xlsx"),"指标统计",&rep); create_xlsx(&rp.join("B.xlsx"),"指标统计",&rep);
    let mut p=mkproj(t.path());
    p.companies=vec![Company{name:"A".into(),business_type:BusinessType::Insurance,regions:vec![]}, Company{name:"B".into(),business_type:BusinessType::Hotel,regions:vec![]}];
    let res=FinancialAggregator.execute(&p).unwrap();
    assert_eq!(res.companies_processed,2);
}

#[test]
fn test_all_previews() {
    let t=tmpdir(); std::fs::create_dir_all(t.path().join("活动量")).unwrap(); std::fs::create_dir_all(t.path().join("经营报表")).unwrap();
    let p=mkproj(t.path());
    assert!(InsuranceAggregator.preview(&p).is_ok());
    assert!(CommercialAggregator.preview(&p).is_ok());
    assert!(HotelAggregator.preview(&p).is_ok());
    let mut p2=p; p2.companies=vec![Company{name:"X".into(),business_type:BusinessType::Insurance,regions:vec![]}];
    assert!(FinancialAggregator.preview(&p2).is_ok());
}
