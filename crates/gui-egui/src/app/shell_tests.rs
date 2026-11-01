use super::tests::{context, finish_job};
use super::*;

#[test]
fn shell_compression_keeps_the_complete_selection_and_never_replaces_output() {
    let context = context();
    let root = tempfile::tempdir().unwrap();
    let sources = [
        root.path().join("中文 空格.txt"),
        root.path().join("--help.txt"),
    ];
    for (index, source) in sources.iter().enumerate() {
        std::fs::write(source, format!("content {index}")).unwrap();
    }
    let mut app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::Compress, sources.to_vec());
    assert!(app.create_open && app.active.is_none());
    assert_eq!(app.draft.sources, sources);
    app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::CompressZip, sources.to_vec());
    finish_job(&mut app, &context);
    let output = root.path().join("中文 空格.zip");
    let original = std::fs::read(&output).unwrap();
    let archive = engine::list(output.clone(), None, &Default::default()).unwrap();
    for source in &sources {
        assert!(archive
            .entries
            .iter()
            .any(|entry| entry.path == source.file_name().unwrap().to_string_lossy()));
    }
    app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::CompressZip, sources.to_vec());
    finish_job(&mut app, &context);
    assert!(app.create_open && app.form_error.is_some());
    assert_eq!(std::fs::read(output).unwrap(), original);
}

#[test]
fn shell_extraction_waits_for_listing_and_uses_the_requested_destination() {
    for action in [Action::Extract, Action::ExtractHere, Action::ExtractFolder] {
        let context = context();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("中文.txt");
        std::fs::write(&source, "archive content").unwrap();
        let mut creator = App::empty(Preferences::default());
        creator.accept_launch(&context, Action::CompressZip, vec![source.clone()]);
        finish_job(&mut creator, &context);
        let archive = root.path().join("中文.zip");
        std::fs::write(&source, "keep existing").unwrap();
        let mut app = App::empty(Preferences::default());
        app.accept_launch(&context, action, vec![archive.clone()]);
        assert!(app.active.is_some());
        finish_job(&mut app, &context);
        if action == Action::Extract {
            let options = app.extraction.as_mut().unwrap();
            assert_eq!(options.destination.path, root.path().join("中文"));
            assert!(!options.destination.path.exists());
            options.destination.set(root.path().join("chosen output"));
            app.begin_extraction(&context);
            finish_job(&mut app, &context);
            assert_eq!(
                std::fs::read_to_string(root.path().join("chosen output/中文.txt")).unwrap(),
                "archive content"
            );
        } else if action == Action::ExtractFolder {
            assert_eq!(
                std::fs::read_to_string(root.path().join("中文/中文.txt")).unwrap(),
                "archive content"
            );
        }
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "keep existing");
        assert!(app.shell_extraction.is_none());
        assert!(!app.history[0].failed);
    }
}

#[test]
fn failed_shell_open_never_extracts_a_different_archive() {
    let context = context();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    std::fs::write(&source, "content").unwrap();
    let mut creator = App::empty(Preferences::default());
    creator.accept_launch(&context, Action::CompressZip, vec![source]);
    finish_job(&mut creator, &context);
    let missing = root.path().join("missing.zip");
    let mut app = App::empty(Preferences::default());
    app.accept_launch(&context, Action::ExtractHere, vec![missing.clone()]);
    finish_job(&mut app, &context);
    assert_eq!(app.retry_path, Some(missing));
    assert!(app.shell_extraction.is_some());
    app.open_archive(&context, root.path().join("source.zip"), None);
    finish_job(&mut app, &context);
    assert!(app.extraction.is_none() && app.history.is_empty());
    assert!(app.shell_extraction.is_none());
}
