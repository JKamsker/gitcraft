//! lcov tracefiles (`lcov.info`, `*.lcov`): `SF:` starts a file, `DA:line,hits` says how often
//! a line ran, `end_of_record` ends the file, and a `TN:` line names the test the records
//! after it are of. Only line hits are read.

use std::collections::HashMap;

use super::{FileCov, Format, Report};

pub fn parse(text: &str) -> Result<Report, String> {
    let mut report = Report {
        format: Format::Lcov,
        ..Report::default()
    };
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut test: Option<usize> = None;
    let mut file: Option<usize> = None;
    let mut records = 0;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(name) = line.strip_prefix("TN:") {
            let name = name.trim();
            test = (!name.is_empty()).then(|| {
                report
                    .tests
                    .iter()
                    .position(|t| t == name)
                    .unwrap_or_else(|| {
                        report.tests.push(name.to_owned());
                        report.tests.len() - 1
                    })
            });
        } else if let Some(path) = line.strip_prefix("SF:") {
            records += 1;
            let path = path.trim().to_owned();
            let at = *by_path.entry(path.clone()).or_insert_with(|| {
                report.files.push(FileCov {
                    path,
                    ..FileCov::default()
                });
                report.files.len() - 1
            });
            file = Some(at);
        } else if let Some(data) = line.strip_prefix("DA:") {
            let Some(at) = file else {
                continue;
            };
            let mut parts = data.split(',');
            let (Some(n), Some(hits)) = (parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(n), Ok(hits)) = (n.trim().parse::<u32>(), hits.trim().parse::<u64>()) else {
                continue;
            };
            let entry = &mut report.files[at];
            let total = entry.lines.entry(n).or_default();
            *total = total.saturating_add(hits);
            if let Some(t) = test
                && hits > 0
            {
                entry.tests.entry(t).or_default().insert(n);
            }
        } else if line == "end_of_record" {
            file = None;
        }
    }
    if records == 0 {
        return Err("not an lcov report (no SF: records)".to_owned());
    }
    Ok(report)
}
