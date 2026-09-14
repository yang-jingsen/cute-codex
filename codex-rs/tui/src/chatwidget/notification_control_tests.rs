use super::*;

#[test]
fn notification_response_is_bound_to_requested_session() {
    assert_eq!(
        parse_response("a", br#"{"thread_id":"a","label":"CIAO!"}"#),
        Ok("CIAO!".into())
    );
    assert!(parse_response("b", br#"{"thread_id":"a","label":"ON"}"#).is_err());
    assert!(parse_response("a", br#"{"thread_id":"a","label":"\u001b[31m"}"#).is_err());
}

#[tokio::test]
async fn notification_item_uses_confirmed_labels_in_preview() {
    let (mut chat, _rx, _op_rx, _sender) =
        crate::chatwidget::tests::make_chatwidget_manual_with_sender().await;
    let mut lines = Vec::new();
    for label in ["CIAO!", "ON", "OFF", "关注", "NOTIFY?"] {
        chat.notification_control.label = Some(label.into());
        let line = chat
            .status_surface_preview_data()
            .status_line_for_items(
                [StatusLineItem::Notification],
                /*use_theme_colors*/ true,
            )
            .unwrap();
        lines.push(
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
        );
    }
    insta::assert_snapshot!(lines.join("\n"), @"
    CIAO!
    ON
    OFF
    关注
    NOTIFY?
    ");
}
