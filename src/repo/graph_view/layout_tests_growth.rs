#[test]
fn compressed_graph_growth_after_author_shrink_uses_description_not_refs() -> Result<(), String> {
    let mut h = exchange_harness(&exchange_settings(), 760.0, false);
    assert_eq!(painted(&h)?.cols.refs, 180.0);
    release_exchange(&mut h, Bar::Author, -130.0)?;
    let start = painted(&h)?;
    assert_eq!(start.cols.author, 150.0);
    assert_eq!(start.cols.description(start.header).width(), 270.0);
    release_exchange(&mut h, Bar::Graph, 10.0)?;
    let after = painted(&h)?;
    eprintln!("Graph sequential exchange: {start:?} -> {after:?}");
    assert_eq!(after.cols.graph, 70.0);
    assert_eq!(after.cols.refs, 180.0);
    assert_eq!(after.cols.author, 150.0);
    assert_eq!(after.cols.description(after.header).width(), 260.0);
    assert_eq!(Settings::get(&h.ctx).refs_width, 280.0);
    Ok(())
}

#[test]
fn compressed_commit_growth_after_date_shrink_uses_description_not_author() -> Result<(), String> {
    let settings = Settings {
        show_hash: true,
        show_date: true,
        ..exchange_settings()
    };
    let mut h = exchange_harness(&settings, 600.0, false);
    release_exchange(&mut h, Bar::Date, -20.0)?;
    let start = painted(&h)?;
    assert_eq!(start.cols.hash, 50.0);
    assert_eq!(start.cols.author, 80.0);
    assert_eq!(start.cols.date, 80.0);
    assert_eq!(start.cols.description(start.header).width(), 160.0);
    // A negative pointer displacement grows a trailing column.
    release_exchange(&mut h, Bar::Hash, 10.0)?;
    let after = painted(&h)?;
    eprintln!("Commit sequential exchange: {start:?} -> {after:?}");
    assert_eq!(after.cols.hash, 60.0);
    assert_eq!(after.cols.author, 80.0);
    assert_eq!(after.cols.date, 80.0);
    assert_eq!(after.cols.description(after.header).width(), 150.0);
    assert_eq!(Settings::get(&h.ctx).author_width, 280.0);
    Ok(())
}
