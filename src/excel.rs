use chrono::Local;
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
use std::fs;
use std::path::Path;

pub struct ExcelExportData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn save_excel(data: &ExcelExportData, results_dir_str: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("results_{}.xlsx", timestamp);
    let trimmed = results_dir_str.trim();
    let results_dir = if trimmed.is_empty() {
        Path::new("results")
    } else {
        Path::new(trimmed)
    };
    if !results_dir.exists() {
        fs::create_dir_all(results_dir)?;
    }
    let filepath = results_dir.join(&filename);

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Results")?;

    let header_format = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x27272a))
        .set_font_color(Color::RGB(0xffffff))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0x3f3f46));

    let index_header_format = Format::new()
        .set_bold()
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_background_color(Color::RGB(0x18181b))
        .set_font_color(Color::RGB(0xffffff))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0x3f3f46));

    let index_cell_format = Format::new()
        .set_align(rust_xlsxwriter::FormatAlign::Center)
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xe4e4e7));

    let cell_format = Format::new()
        .set_text_wrap()
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xe4e4e7));

    let success_format = Format::new()
        .set_bold()
        .set_text_wrap()
        .set_font_color(Color::RGB(0x15803d))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xe4e4e7));

    let failed_format = Format::new()
        .set_bold()
        .set_text_wrap()
        .set_font_color(Color::RGB(0xb91c1c))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xe4e4e7));

    let link_format = Format::new()
        .set_font_color(Color::RGB(0x2563eb))
        .set_underline(rust_xlsxwriter::FormatUnderline::Single)
        .set_text_wrap()
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xe4e4e7));

    worksheet.write_string_with_format(0, 0, "#", &index_header_format)?;
    worksheet.set_column_width(0, 6.0)?;

    for (col_idx, h) in data.headers.iter().enumerate() {
        worksheet.write_string_with_format(0, (col_idx + 1) as u16, h, &header_format)?;
    }

    let mut max_lens = vec![10usize; data.headers.len()];
    for (i, h) in data.headers.iter().enumerate() {
        max_lens[i] = max_lens[i].max(h.len());
    }

    for (row_idx, row) in data.rows.iter().enumerate() {
        let r = (row_idx + 1) as u32;
        let index_num = (row_idx + 1) as u32;
        worksheet.write_number_with_format(r, 0, index_num, &index_cell_format)?;

        for (col_idx, val) in row.iter().enumerate() {
            let c = (col_idx + 1) as u16;
            if col_idx < max_lens.len() {
                let line_len = val.lines().map(|l| l.len()).max().unwrap_or(val.len());
                max_lens[col_idx] = max_lens[col_idx].max(line_len);
            }

            if col_idx == 1 {
                if val.starts_with("Success") {
                    worksheet.write_string_with_format(r, c, val, &success_format)?;
                } else if val.starts_with("Failed") {
                    worksheet.write_string_with_format(r, c, val, &failed_format)?;
                } else {
                    worksheet.write_string_with_format(r, c, val, &cell_format)?;
                }
            } else if val.starts_with("backups/") && val.ends_with(".tar.gz") {
                if let Err(_) = worksheet.write_url_with_format(r, c, val.as_str(), &link_format) {
                    worksheet.write_string_with_format(r, c, val, &cell_format)?;
                }
            } else {
                worksheet.write_string_with_format(r, c, val, &cell_format)?;
            }
        }
    }

    for (col_idx, max_len) in max_lens.iter().enumerate() {
        let width = (*max_len as f64 + 3.0).min(50.0);
        worksheet.set_column_width((col_idx + 1) as u16, width)?;
    }

    worksheet.set_freeze_panes(1, 0)?;
    if !data.rows.is_empty() {
        let last_row = data.rows.len() as u32;
        let last_col = data.headers.len() as u16;
        worksheet.autofilter(0, 0, last_row, last_col)?;
    }

    workbook.save(&filepath)?;
    Ok(filename)
}
