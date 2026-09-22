use serde::{Deserialize, Serialize};

/// 与 src/platform/types.ts 的 NotificationItem 一一对应
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationItem {
    /// Notification.Id 单调递增 去重水位
    pub id: i64,
    pub aumid: String,
    /// HandlerAssets.DisplayName 缺省回退 AUMID
    pub app: String,
    /// HandlerAssets.IconUri 无 URI 时为 aumid: 前缀 由 notify_image 反查
    pub icon: String,
    /// toast 内嵌图 多为本地路径 优先于 icon
    pub image: String,
    pub title: String,
    pub body: String,
    /// toast 深链参数 激活时回灌
    pub launch: String,
    /// foreground|background|protocol protocol 时 launch 为 URI
    pub atype: String,
    /// unix ms
    pub arrival: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToastPayload {
    pub launch: String,
    pub atype: String,
    pub title: String,
    pub body: String,
    pub image: String,
}

pub fn parse_toast_payload(xml: &str) -> ToastPayload {
    let mut payload = ToastPayload::default();
    let mut texts: Vec<String> = Vec::new();
    let mut images: Vec<(String, String)> = Vec::new();

    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        // 跳过声明注释闭合标签
        if rest.starts_with('?') || rest.starts_with('!') || rest.starts_with('/') {
            match rest.find('>') {
                Some(gt) => rest = &rest[gt + 1..],
                None => break,
            }
            continue;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = &rest[name_end..];

        let (attrs, self_closed, after) = parse_attrs(rest);
        rest = after;

        match name {
            "toast" => {
                payload.launch = attr(&attrs, "launch").unwrap_or_default();
                payload.atype = attr(&attrs, "activationType").unwrap_or_default();
            }
            "text" if !self_closed => {
                if let Some(end) = rest.find("</text") {
                    let value = unescape(rest[..end].trim());
                    if !value.is_empty() {
                        texts.push(value);
                    }
                    rest = &rest[end..];
                } else {
                    break;
                }
            }
            "image" => {
                let src = attr(&attrs, "src").unwrap_or_default();
                let placement = attr(&attrs, "placement").unwrap_or_default();
                images.push((src, placement));
            }
            _ => {}
        }
    }

    if let Some(first) = texts.first() {
        payload.title = first.clone();
    }
    if texts.len() >= 2 {
        payload.body = texts[1..].join("  ");
    }
    let avatar = images
        .iter()
        .find(|(_, placement)| placement.eq_ignore_ascii_case("appLogoOverride"));
    payload.image = avatar
        .or_else(|| images.first())
        .map(|(src, _)| src.clone())
        .unwrap_or_default();
    payload
}

fn attr(attrs: &[(&str, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.clone())
}

fn parse_attrs(input: &str) -> (Vec<(&str, String)>, bool, &str) {
    let mut attrs = Vec::new();
    let mut rest = input;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return (attrs, false, rest);
        }
        if let Some(after) = rest.strip_prefix("/>") {
            return (attrs, true, after);
        }
        if let Some(after) = rest.strip_prefix('>') {
            return (attrs, false, after);
        }
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = &rest[name_end..];
        let Some(after) = rest.trim_start().strip_prefix('=') else {
            // 无值属性 toast XML 不出现
            continue;
        };
        rest = after.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            return (attrs, false, rest);
        };
        rest = &rest[quote.len_utf8()..];
        match rest.find(quote) {
            Some(end) => {
                if !name.is_empty() {
                    attrs.push((name, unescape(&rest[..end])));
                }
                rest = &rest[end + 1..];
            }
            None => return (attrs, false, ""),
        }
    }
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let Some(semi) = rest.find(';') else {
            out.push('&');
            out.push_str(rest);
            return out;
        };
        let entity = &rest[..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => decode_numeric(entity),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                // 未知实体原样保留
                out.push('&');
                out.push_str(entity);
                out.push(';');
                rest = &rest[semi + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_numeric(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let value = if let Some(hex) = digits
        .strip_prefix('x')
        .or_else(|| digits.strip_prefix('X'))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    char::from_u32(value)
}

#[cfg(test)]
mod tests {
    use super::{parse_toast_payload, unescape};

    #[test]
    fn parse_extracts_all_fields_from_a_realistic_wechat_style_toast() {
        let xml = r##"<toast launch="weixin://chat?id=wxid_abc&amp;type=1" activationType="protocol">
  <visual>
    <binding template="ToastGeneric">
      <text>张三</text>
      <text>晚上一起吃饭？</text>
      <image placement="appLogoOverride" hint-crop="circle" src="C:\Users\me\AppData\Local\Packages\Tencent.WeChat_abc\LocalCache\avatar.png"/>
    </binding>
  </visual>
</toast>"##;
        let p = parse_toast_payload(xml);
        assert_eq!(p.launch, "weixin://chat?id=wxid_abc&type=1");
        assert_eq!(p.atype, "protocol");
        assert_eq!(p.title, "张三");
        assert_eq!(p.body, "晚上一起吃饭？");
        assert_eq!(
            p.image,
            r"C:\Users\me\AppData\Local\Packages\Tencent.WeChat_abc\LocalCache\avatar.png"
        );
    }

    #[test]
    fn parse_joins_multiple_body_lines_with_two_spaces() {
        let xml = r#"<toast launch="x"><visual><binding template="ToastGeneric">
          <text>群聊</text><text>李四: 收到</text><text>王五: 好的</text>
        </binding></visual></toast>"#;
        let p = parse_toast_payload(xml);
        assert_eq!(p.title, "群聊");
        assert_eq!(p.body, "李四: 收到  王五: 好的");
    }

    #[test]
    fn parse_prefers_applogooverride_over_hero_image() {
        let xml = r#"<toast><visual><binding template="ToastGeneric">
          <image src="hero.png"/>
          <image placement="appLogoOverride" src="avatar.jpg"/>
          <text>t</text>
        </binding></visual></toast>"#;
        let p = parse_toast_payload(xml);
        assert_eq!(p.image, "avatar.jpg");
    }

    #[test]
    fn parse_falls_back_to_first_image_when_no_avatar() {
        let xml = r#"<toast><visual><binding template="ToastImageAndText02">
          <image src="http://example.com/a.png"/><text>t</text><text>b</text>
        </binding></visual></toast>"#;
        let p = parse_toast_payload(xml);
        assert_eq!(p.image, "http://example.com/a.png");
    }

    #[test]
    fn parse_skips_empty_text_elements() {
        let xml = r#"<toast><visual><binding template="ToastGeneric">
          <text></text><text>  </text><text>真正的标题</text><text>正文</text>
        </binding></visual></toast>"#;
        let p = parse_toast_payload(xml);
        assert_eq!(p.title, "真正的标题");
        assert_eq!(p.body, "正文");
    }

    #[test]
    fn parse_handles_single_quoted_attributes() {
        let xml = "<toast launch='myapp://open' activationType='background'><visual><binding template='ToastGeneric'><text>t</text></binding></visual></toast>";
        let p = parse_toast_payload(xml);
        assert_eq!(p.launch, "myapp://open");
        assert_eq!(p.atype, "background");
    }

    #[test]
    fn parse_tolerates_truncated_xml_without_panicking() {
        let p = parse_toast_payload(r#"<toast launch="broken"#);
        assert_eq!(p.launch, "");
        let p = parse_toast_payload(r#"<toast launch="ok"><visual><binding><text>没闭合"#);
        assert_eq!(p.launch, "ok");
        assert_eq!(p.title, "");
    }

    #[test]
    fn parse_returns_empty_payload_for_empty_input() {
        let p = parse_toast_payload("");
        assert_eq!(p, Default::default());
        let p = parse_toast_payload("not xml at all");
        assert_eq!(p, Default::default());
    }

    #[test]
    fn parse_unescapes_entities_in_text() {
        let xml = r"<toast><visual><binding><text>Tom &amp; Jerry &lt;3 &#65;&#x42;</text></binding></visual></toast>";
        let p = parse_toast_payload(xml);
        assert_eq!(p.title, "Tom & Jerry <3 AB");
    }

    #[test]
    fn unescape_keeps_unknown_entities_verbatim() {
        assert_eq!(unescape("a &bogus; b"), "a &bogus; b");
        assert_eq!(unescape("100% & rest"), "100% & rest");
    }
}
