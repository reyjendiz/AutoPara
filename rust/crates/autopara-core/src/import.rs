//! Importing a timetable: read it, keep our own copy, store it (PORT_SPEC §B.2, plan step 2.6).
//!
//! The document the user picks is usually a download that gets tidied away, so AutoPara keeps one
//! copy of its own and re-imports from that. A new document replaces the old records, keeps manual
//! lessons and the chosen course and group, forgets every verdict, and applies only from now.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::db::{DbError, Storage};
use crate::docx::DocxError;
use crate::parser::{ParsedCourse, parse_file};

/// The new app's files live beside the Python app's in the same folder, under names of their
/// own, so the old data is never touched (see DECISIONS.md, "Data folder").
pub const DATABASE_FILE: &str = "autopara2.db";
pub const TIMETABLES_DIR: &str = "timetables";

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error(transparent)]
    Document(#[from] DocxError),
    #[error(transparent)]
    Database(#[from] DbError),
    #[error("could not keep a copy of the timetable: {0}")]
    Copy(#[from] std::io::Error),
    #[error("the document has no timetable tables")]
    NoTables,
}

/// `%APPDATA%\AutoPara` on Windows, `~/Library/Application Support/AutoPara` on macOS — the
/// Python app's folder (PORT_SPEC §B.3).
pub fn default_data_root() -> Option<PathBuf> {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(appdata).join("AutoPara"));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Application Support/AutoPara"))
    } else {
        Some(home.join("AutoPara"))
    }
}

/// Parse a document for the import dialog, with the user's class length (the Python app always
/// used 80 minutes here; see DECISIONS.md).
pub fn read_timetable(path: &Path, class_minutes: u32) -> Result<Vec<ParsedCourse>, ImportError> {
    let courses = parse_file(path, class_minutes)?;
    if courses.is_empty() {
        return Err(ImportError::NoTables);
    }
    Ok(courses)
}

/// Keep exactly one copy of `source` in `<data_root>/timetables` and return its path. Re-importing
/// the copy itself is ordinary, so the sweep never deletes the file it is about to read.
pub fn keep_copy(data_root: &Path, source: &Path) -> Result<PathBuf, std::io::Error> {
    let directory = data_root.join(TIMETABLES_DIR);
    fs::create_dir_all(&directory)?;
    let source = fs::canonicalize(source)?;
    let name = source
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;
    let destination = directory.join(name);
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.is_file() && fs::canonicalize(&path)? != source {
            fs::remove_file(&path)?;
        }
    }
    if fs::canonicalize(&destination).ok().as_deref() != Some(source.as_path()) {
        fs::copy(&source, &destination)?;
    }
    Ok(destination)
}

/// Store `courses` (already read from `source`) and remember where the document came from.
pub fn import_timetable(
    storage: &mut Storage,
    data_root: &Path,
    source: &Path,
    courses: &[ParsedCourse],
    now: NaiveDateTime,
) -> Result<PathBuf, ImportError> {
    let copy = keep_copy(data_root, source)?;
    storage.import_courses(courses, now)?;
    storage.set_setting("last_import_path", &source.to_string_lossy())?;
    storage.set_setting("schedule_copy_path", &copy.to_string_lossy())?;
    Ok(copy)
}

/// Choose the course (by ordinal) and group (by name) to show; `false` when either is missing.
pub fn select_group(storage: &Storage, ordinal: u32, group_name: &str) -> Result<bool, DbError> {
    let Some(course) = storage
        .courses()?
        .into_iter()
        .find(|course| course.ordinal == ordinal)
    else {
        return Ok(false);
    };
    let Some(group) = storage
        .groups(course.id)?
        .into_iter()
        .find(|group| group.name == group_name)
    else {
        return Ok(false);
    };
    storage.set_setting("selected_course_id", &course.id.to_string())?;
    storage.set_setting("selected_group_id", &group.id.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::parse_datetime;

    fn sample() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/sample_timetable.docx")
    }

    #[test]
    fn import_copies_the_document_and_keeps_the_selection() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = outside.path().join("розклад.docx");
        fs::copy(sample(), &source).unwrap();

        let mut storage = Storage::open_in_memory().unwrap();
        let courses = read_timetable(&source, 90).unwrap();
        assert_eq!(
            courses[0].lessons[1].end.to_string(),
            "11:00",
            "the 09:30 class lasts the 90 minutes asked for"
        );
        let now = parse_datetime("2026-10-03T15:00:00").unwrap();
        let copy = import_timetable(&mut storage, data.path(), &source, &courses, now).unwrap();
        assert!(select_group(&storage, 1, "А2-Бд26-12 група").unwrap());
        let chosen = storage.settings().unwrap().selected_group_id;

        // The original goes away; re-importing from our copy works and keeps the choice.
        fs::remove_file(&source).unwrap();
        let again = read_timetable(&copy, 80).unwrap();
        let copy_again = import_timetable(&mut storage, data.path(), &copy, &again, now).unwrap();
        assert_eq!(
            fs::canonicalize(copy_again).unwrap(),
            fs::canonicalize(&copy).unwrap()
        );
        assert_eq!(storage.settings().unwrap().selected_group_id, chosen);
        assert_eq!(
            fs::read_dir(data.path().join(TIMETABLES_DIR))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn a_new_document_replaces_the_old_copy() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let (first, second) = (outside.path().join("a.docx"), outside.path().join("b.docx"));
        fs::copy(sample(), &first).unwrap();
        fs::copy(sample(), &second).unwrap();
        keep_copy(data.path(), &first).unwrap();
        let kept = keep_copy(data.path(), &second).unwrap();
        let names: Vec<_> = fs::read_dir(data.path().join(TIMETABLES_DIR))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [kept.file_name().unwrap().to_owned()]);
        assert!(first.exists(), "the user's own file is never touched");
    }

    #[test]
    fn unknown_selection_is_refused() {
        let storage = Storage::open_in_memory().unwrap();
        assert!(!select_group(&storage, 1, "nope").unwrap());
    }
}
