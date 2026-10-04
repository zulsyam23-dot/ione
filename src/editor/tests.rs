
    use super::*;
    use eframe::egui;
    use egui::TextBuffer;
    use egui::text::LayoutJob;

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn ctrl_key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }
    }

    fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn setup(ctx: &egui::Context) {
        ctx.set_fonts(egui::FontDefinitions::default());
    }

    #[test]
    fn painted_editor_renders_rainbow_brackets() {
        let ctx = egui::Context::default();
        setup(&ctx);

        let mut tab = Tab::new(
            "main.rs",
            "fn f() {\n    let v = vec![1, (2 + 3), {4}];\n    g(v);\n}\n",
            egui_code_editor::Syntax::rust(),
        );
        let overlay = EditorOverlay {
            bracket_guides: true,
            colorize_brackets: true,
        };
        let palette = Palette::dark();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                show_editor(
                    ui,
                    &mut tab,
                    Theme::GithubDark,
                    "000000",
                    &overlay,
                    &palette,
                    &mut Icons::new(),
                );
            });
        });

        let mut colors = std::collections::BTreeSet::new();
        for cs in &output.shapes {
            if let egui::Shape::Text(ts) = &cs.shape {
                for sec in &ts.galley.job.sections {
                    colors.insert(sec.format.color.to_array());
                }
            }
        }
        output.drop_without_applying_deltas();

        assert!(colors.len() >= 4, "painted colors = {colors:?}");
    }

    #[test]
    fn editor_paints_squiggles_for_diagnostics() {
        // A file with an unmatched bracket/trailing whitespace must paint
        // squiggle line segments; a clean file must not.
        let run = |source: &str| {
            let ctx = egui::Context::default();
            setup(&ctx);
            let mut tab = Tab::new("main.rs", source, egui_code_editor::Syntax::rust());
            let overlay = EditorOverlay {
                bracket_guides: true,
                colorize_brackets: true,
            };
            let palette = Palette::dark();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    show_editor(
                        ui,
                        &mut tab,
                        Theme::GithubDark,
                        "000000",
                        &overlay,
                        &palette,
                        &mut Icons::new(),
                    );
                });
            });
            let segments = output
                .shapes
                .iter()
                .filter(|cs| matches!(cs.shape, egui::Shape::LineSegment { .. }))
                .count();
            output.drop_without_applying_deltas();
            segments
        };

        let bad = run("fn f() {\n    let x = (1;\n}\n");
        let clean = run("fn f() {\n    let x = 1;\n}\n");
        assert!(bad > 0, "expect squiggles for an unclosed paren, got {bad}");
        assert_eq!(clean, 0, "a clean file must not paint squiggles");
    }

    fn frame(ctx: &egui::Context, text: &mut String, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut layouter = |ui: &Ui, text_buffer: &dyn TextBuffer, _wrap_width: f32| {
                    let job = LayoutJob::single_section(
                        text_buffer.as_str().to_string(),
                        styling::format_font(FONT_SIZE, egui::Color32::WHITE),
                    );
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let te = egui::TextEdit::multiline(&mut *text)
                    .id_source("repro")
                    .lock_focus(true)
                    .desired_rows(TEXT_ROWS)
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE)
                    .layouter(&mut layouter);
                te.show(ui);
            });
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn editing_gestures_do_not_panic() {
        let ctx = egui::Context::default();
        setup(&ctx);

        let mut text = String::from("fn main() {\n    let x = 1;\n}\n");
        let big = "fn f() {\n  [1, 2, 3].iter().map(|v| v * v).collect()\n}\n\n".repeat(30);
        let steps = vec![
            vec![
                click(egui::pos2(120.0, 100.0), true),
                click(egui::pos2(120.0, 100.0), false),
            ],
            vec![egui::Event::Paste(big.clone())],
            vec![egui::Event::Text("xyz".to_string())],
            vec![
                key(egui::Key::Backspace, true),
                key(egui::Key::Backspace, false),
            ],
            vec![key(egui::Key::Enter, true), key(egui::Key::Enter, false)],
            vec![ctrl_key(egui::Key::A)],
            vec![egui::Event::Paste(
                "  } else if (a) {\n    b();\n  }\n".to_string(),
            )],
            vec![
                key(egui::Key::ArrowUp, true),
                key(egui::Key::ArrowUp, false),
            ],
            vec![
                key(egui::Key::ArrowDown, true),
                key(egui::Key::ArrowDown, false),
            ],
            vec![key(egui::Key::Delete, true), key(egui::Key::Delete, false)],
            vec![
                key(egui::Key::ArrowLeft, true),
                ctrl_key(egui::Key::Backspace),
            ],
            vec![key(egui::Key::Home, true), ctrl_key(egui::Key::Home)],
            vec![
                click(egui::pos2(20.0, 60.0), true),
                click(egui::pos2(700.0, 200.0), false),
            ],
            vec![egui::Event::Text("overwritten".to_string())],
            vec![egui::Event::Paste(String::new())],
            vec![egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                before_chars: 100,
                after_chars: 0,
            })],
            vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "test".to_string(),
                active_range_chars: None,
            })],
            vec![egui::Event::Ime(egui::ImeEvent::Commit("done".to_string()))],
            vec![ctrl_key(egui::Key::Z), ctrl_key(egui::Key::Y)],
            vec![key(egui::Key::Tab, true), key(egui::Key::Tab, false)],
        ];

        for events in steps {
            frame(&ctx, &mut text, events);
        }
    }

    fn folded_frame(ctx: &egui::Context, tab: &mut Tab, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let overlay = EditorOverlay {
            bracket_guides: true,
            colorize_brackets: true,
        };
        let palette = Palette::dark();
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                show_editor(
                    ui,
                    tab,
                    Theme::GithubDark,
                    "000000",
                    &overlay,
                    &palette,
                    &mut Icons::new(),
                );
            });
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn fold_interactions_do_not_panic() {
        let ctx = egui::Context::default();
        setup(&ctx);

        let mut tab = Tab::new(
            "main.rs",
            "fn outer() {\n    fn inner() {\n        let x = 1;\n        g(x);\n    }\n    let y = 2;\n    h(y);\n}\nfn tail() {\n    step();\n}\n",
            egui_code_editor::Syntax::rust(),
        );

        // Fold `outer` (opening brace on line 0) and `inner` (line 1).
        let reals: Vec<char> = tab.content.chars().collect();
        let (mask, _) = styling::mask_and_links(&tab.content, &tab.syntax);
        let scan = crate::editor::guides::analyze_brackets(&reals, &mask);
        let opens: Vec<usize> = scan
            .brace_pairs
            .iter()
            .filter(|b| b.close != usize::MAX)
            .map(|b| b.open)
            .collect();
        folds::toggle_fold(&mut tab.folds, &opens, &tab.content, 0);
        folds::toggle_fold(&mut tab.folds, &opens, &tab.content, 1);

        // Render with both folds closed.
        folded_frame(&ctx, &mut tab, vec![]);

        let steps: Vec<Vec<egui::Event>> = vec![
            // Type at the caret.
            vec![egui::Event::Text("x".to_string())],
            // Select-all + replace across folded lines.
            vec![
                ctrl_key(egui::Key::A),
                egui::Event::Text("fn z() {\n    a();\n}\n".to_string()),
            ],
            // Rewrite content fully inside the fold region (past the marker).
            vec![
                ctrl_key(egui::Key::A),
                egui::Event::Paste(
                    "fn outer() {\n    fn inner() {\n        let x = 1;\n    }\n    y();\n}\n"
                        .to_string(),
                ),
            ],
            // Toggle `outer` unfolded again, then re-fold.
            vec![],
        ];
        for events in &steps {
            folded_frame(&ctx, &mut tab, events.clone());
        }
        let reals: Vec<char> = tab.content.chars().collect();
        let (mask, _) = styling::mask_and_links(&tab.content, &tab.syntax);
        let scan = crate::editor::guides::analyze_brackets(&reals, &mask);
        let opens: Vec<usize> = scan
            .brace_pairs
            .iter()
            .filter(|b| b.close != usize::MAX)
            .map(|b| b.open)
            .collect();
        folds::toggle_fold(&mut tab.folds, &opens, &tab.content, 0);
        folded_frame(&ctx, &mut tab, vec![]);

        // Jump to a hidden line (line 3 of real content is inside a fold).
        tab.goto_line = Some(4);
        folded_frame(&ctx, &mut tab, vec![]);

        // Fold everything that remains and render.
        for li in 0..8 {
            folds::toggle_fold(&mut tab.folds, &opens, &tab.content, li);
        }
        folded_frame(&ctx, &mut tab, vec![]);
        for li in 0..8 {
            folds::toggle_fold(&mut tab.folds, &opens, &tab.content, li);
        }
        folded_frame(&ctx, &mut tab, vec![]);
    }
