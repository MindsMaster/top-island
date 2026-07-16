using System;
using System.Linq;
using System.Text.RegularExpressions;
using System.Xml.Linq;

namespace TopIsland.WinBridge
{
    /// <summary>
    /// 从 toast payload XML 提取 launch/activationType/标题/正文/图片。image：聊天类应用
    /// （微信/QQ/Phone Link）把发信人头像放在 &lt;image placement="appLogoOverride"&gt;，优先取它。
    /// </summary>
    internal sealed class ToastPayload
    {
        public string Launch = "";
        public string AType = "";
        public string Title = "";
        public string Body = "";
        public string Image = "";

        public static ToastPayload Parse(string xmlText)
        {
            var result = new ToastPayload();
            if (string.IsNullOrWhiteSpace(xmlText)) return result;
            try
            {
                var doc = XDocument.Parse(xmlText);
                var root = doc.Root;
                if (root != null && root.Name.LocalName == "toast")
                {
                    result.Launch = (string)root.Attribute("launch") ?? "";
                    result.AType = (string)root.Attribute("activationType") ?? "";
                }

                var texts = doc.Descendants()
                    .Where(e => e.Name.LocalName == "text")
                    .Select(e => e.Value.Trim())
                    .Where(v => v.Length > 0)
                    .ToList();
                if (texts.Count >= 1) result.Title = texts[0];
                if (texts.Count >= 2) result.Body = string.Join("  ", texts.Skip(1));

                var images = doc.Descendants().Where(e => e.Name.LocalName == "image").ToList();
                var avatar = images.FirstOrDefault(e => string.Equals(
                    (string)e.Attribute("placement"), "appLogoOverride", StringComparison.OrdinalIgnoreCase));
                result.Image = (string)(avatar ?? images.FirstOrDefault())?.Attribute("src") ?? "";
            }
            catch
            {
                // XML 破损时退化为正则
                result.Launch = Match1(xmlText, "launch=\"([^\"]*)\"");
                result.AType = Match1(xmlText, "activationType=\"([^\"]*)\"");
                var texts = Regex.Matches(xmlText, "<text[^>]*>(.*?)</text>");
                if (texts.Count >= 1) result.Title = texts[0].Groups[1].Value;
                if (texts.Count >= 2) result.Body = texts[1].Groups[1].Value;
                result.Image = Match1(xmlText, "<image[^>]*src=\"([^\"]*)\"");
            }
            return result;
        }

        private static string Match1(string text, string pattern)
        {
            var m = Regex.Match(text, pattern);
            return m.Success ? m.Groups[1].Value : "";
        }
    }
}
