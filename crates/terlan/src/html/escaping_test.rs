use super::*;

/// Verifies attribute escaping covers attribute-sensitive characters.
///
/// Inputs:
/// - Text containing a quote, angle brackets, and an ampersand.
///
/// Output:
/// - Attribute-safe HTML entity text.
///
/// Transformation:
/// - Pins the static-template attribute escaping boundary shared by CLI
///   renderers.
#[test]
fn escape_html_attr_escapes_attribute_sensitive_characters() {
    assert_eq!(escape_html_attr("\"<admin>&"), "&quot;&lt;admin&gt;&amp;");
}

/// Verifies text-node escaping delegates to the shared sanitizer.
///
/// Inputs:
/// - Text containing tags, ampersands, spaces, and a closing tag marker.
///
/// Output:
/// - Text-node-safe HTML entity text.
///
/// Transformation:
/// - Pins the current `ammonia::clean_text` behavior used by static templates.
#[test]
fn escape_html_text_uses_ammonia_text_escaping() {
    assert_eq!(
        escape_html_text("Hi & </script>"),
        "Hi&#32;&amp;&#32;&lt;&#47;script&gt;"
    );
}
