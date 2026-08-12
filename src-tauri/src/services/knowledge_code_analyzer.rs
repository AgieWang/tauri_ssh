use regex::Regex;

/// 所有源码分析器都遵守这个稳定边界；调用方只依赖统一结果，不依赖具体解析库。
pub trait LanguageAnalyzer {
    fn language(&self) -> &'static str;
    fn analyze(&self, path: &str, content: &str) -> CodeAnalysisResult;
}

/// P0 语言的统一、可替换分析入口。
///
/// 当前实现刻意使用结构化降级分析，避免在未完成三平台依赖 Spike 前引入不可控的
/// Tree-sitter/原生绑定；结果会如实标记为 `structured_fallback`，不得伪称 AST 精度。
pub struct P0LanguageAnalyzer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAnalysisResult {
    pub language: String,
    pub analysis_level: String,
    pub parser_error: Option<String>,
    pub symbols: Vec<AnalyzedCodeSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedCodeSymbol {
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub signature: String,
    pub start_line: i64,
    pub end_line: i64,
}

impl P0LanguageAnalyzer {
    pub fn detect_language(path: &str) -> Option<&'static str> {
        let normalized = path.to_ascii_lowercase();
        if normalized.ends_with(".rs") {
            Some("rust")
        } else if normalized.ends_with(".ts")
            || normalized.ends_with(".tsx")
            || normalized.ends_with(".js")
            || normalized.ends_with(".jsx")
        {
            Some("typescript")
        } else if normalized.ends_with(".vue") {
            Some("vue")
        } else if normalized.ends_with(".java") {
            Some("java")
        } else if normalized.ends_with(".sql") {
            Some("sql")
        } else if normalized.ends_with(".xml") {
            Some("mybatis_xml")
        } else if normalized.ends_with(".md")
            || normalized.ends_with(".mdx")
            || normalized.ends_with(".markdown")
            || normalized.ends_with(".mdown")
            || normalized.ends_with(".mkdn")
        {
            Some("markdown")
        } else {
            None
        }
    }

    pub fn analyze_path(path: &str, content: &str) -> CodeAnalysisResult {
        match Self::detect_language(path) {
            Some("rust") => RustAnalyzer.analyze(path, content),
            Some("typescript") => TypeScriptAnalyzer.analyze(path, content),
            Some("vue") => VueAnalyzer.analyze(path, content),
            Some("java") => JavaAnalyzer.analyze(path, content),
            Some("sql") => SqlAnalyzer.analyze(path, content),
            Some("mybatis_xml") => MyBatisXmlAnalyzer.analyze(path, content),
            Some("markdown") => MarkdownAnalyzer.analyze(path, content),
            _ => CodeAnalysisResult {
                language: "unknown".to_string(),
                analysis_level: "skipped".to_string(),
                parser_error: Some("unsupported_language".to_string()),
                symbols: Vec::new(),
            },
        }
    }
}

struct RustAnalyzer;
struct TypeScriptAnalyzer;
struct VueAnalyzer;
struct JavaAnalyzer;
struct SqlAnalyzer;
struct MyBatisXmlAnalyzer;
struct MarkdownAnalyzer;

impl LanguageAnalyzer for MarkdownAnalyzer {
    fn language(&self) -> &'static str {
        "markdown"
    }

    fn analyze(&self, _path: &str, content: &str) -> CodeAnalysisResult {
        // Markdown 的结构证据只提取标题、链接和代码块标记；正文仍由结构化
        // Markdown parser 分块，避免把文档伪装成代码快照证据。
        analyze_with_patterns(
            self.language(),
            content,
            &[
                ("heading", r"(?m)^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$"),
                ("link", r"\[[^\]]+\]\(([^)]+)\)"),
                (
                    "code_block",
                    r"(?m)^\s*(?:```|~~~)\s*([A-Za-z0-9_+.-]+)\s*$",
                ),
            ],
        )
    }
}

impl LanguageAnalyzer for RustAnalyzer {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn analyze(&self, _path: &str, content: &str) -> CodeAnalysisResult {
        analyze_with_patterns(
            self.language(),
            content,
            &[
                (
                    "function",
                    r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)[^\n{;]*",
                ),
                (
                    "struct",
                    r"(?m)^\s*(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "enum",
                    r"(?m)^\s*(?:pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "trait",
                    r"(?m)^\s*(?:pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "module",
                    r"(?m)^\s*(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "command",
                    r"(?ms)#\s*\[\s*tauri::command\s*\]\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "model",
                    r"(?ms)#\s*\[\s*derive\([^\]]*(?:Serialize|Deserialize)[^\]]*\)\s*\]\s*(?:pub\s+)?(?:struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "test",
                    r"(?ms)#\s*\[\s*(?:tokio::)?test\s*\][^{;]*?(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "config_key",
                    r#"(?m)\b(?:var|env)\s*\(\s*[\"']([^\"']+)[\"']"#,
                ),
            ],
        )
    }
}

impl LanguageAnalyzer for TypeScriptAnalyzer {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn analyze(&self, _path: &str, content: &str) -> CodeAnalysisResult {
        analyze_with_patterns(
            self.language(),
            content,
            &[
                (
                    "class",
                    r"(?m)^\s*(?:export\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                ),
                (
                    "interface",
                    r"(?m)^\s*(?:export\s+)?interface\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                ),
                (
                    "function",
                    r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)[^\n{;]*",
                ),
                (
                    "component",
                    r"(?m)^\s*(?:export\s+)?const\s+([A-Z][A-Za-z0-9_$]*)\s*=",
                ),
                (
                    "route",
                    r#"(?im)\b(?:get|post|put|delete|patch)\s*\(\s*[\"'](/[^\"']+)"#,
                ),
                ("ipc_command", r#"\binvoke\s*\(\s*[\"']([^\"']+)"#),
                (
                    "test",
                    r#"(?im)\b(?:it|test|describe)\s*\(\s*[\"']([^\"']+)"#,
                ),
                (
                    "config_key",
                    r#"(?im)\b(?:getItem|setItem|load)\s*\(\s*[\"']([^\"']+)"#,
                ),
            ],
        )
    }
}

impl LanguageAnalyzer for VueAnalyzer {
    fn language(&self) -> &'static str {
        "vue"
    }

    fn analyze(&self, path: &str, content: &str) -> CodeAnalysisResult {
        let mut result = TypeScriptAnalyzer.analyze(path, content);
        result.language = self.language().to_string();
        if let Some(name) = path
            .rsplit('/')
            .next()
            .and_then(|file| file.strip_suffix(".vue"))
        {
            result.symbols.push(AnalyzedCodeSymbol {
                kind: "component".to_string(),
                name: name.to_string(),
                qualified_name: name.to_string(),
                signature: format!("<{}>", name),
                start_line: 1,
                end_line: content.lines().count().max(1) as i64,
            });
            result.analysis_level = "structured_fallback".to_string();
        }
        result
    }
}

impl LanguageAnalyzer for JavaAnalyzer {
    fn language(&self) -> &'static str {
        "java"
    }

    fn analyze(&self, _path: &str, content: &str) -> CodeAnalysisResult {
        analyze_with_patterns(
            self.language(),
            content,
            &[
                (
                    "class",
                    r"(?m)^\s*(?:public|protected|private)?\s*(?:abstract\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "interface",
                    r"(?m)^\s*(?:public\s+)?interface\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "enum",
                    r"(?m)^\s*(?:public\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)",
                ),
                (
                    "method",
                    r"(?m)^\s*(?:public|protected|private)\s+(?:static\s+)?[A-Za-z_][A-Za-z0-9_<>, ?\[\]]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
                ),
                (
                    "route",
                    r#"(?im)@(?:Get|Post|Put|Delete|Patch)Mapping\s*\(\s*(?:value\s*=\s*)?[\"']?(/[^\"')]+)"#,
                ),
                (
                    "feign_client",
                    r#"(?im)@FeignClient\s*\(\s*(?:name\s*=\s*)?[\"']([^\"')]+)"#,
                ),
                (
                    "test",
                    r"(?ms)@Test\s*(?:public\s+)?(?:void|[A-Za-z_][A-Za-z0-9_<>]*)\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
                ),
            ],
        )
    }
}

impl LanguageAnalyzer for SqlAnalyzer {
    fn language(&self) -> &'static str {
        "sql"
    }

    fn analyze(&self, _path: &str, content: &str) -> CodeAnalysisResult {
        analyze_with_patterns(
            self.language(),
            content,
            &[
                (
                    "table",
                    r"(?im)\bcreate\s+table\s+(?:if\s+not\s+exists\s+)?`?([A-Za-z_][A-Za-z0-9_]*)`?",
                ),
                (
                    "view",
                    r"(?im)\bcreate\s+(?:or\s+replace\s+)?view\s+`?([A-Za-z_][A-Za-z0-9_]*)`?",
                ),
                (
                    "procedure",
                    r"(?im)\bcreate\s+(?:procedure|function)\s+`?([A-Za-z_][A-Za-z0-9_]*)`?",
                ),
                (
                    "column",
                    r"(?im)^\s*`?([A-Za-z_][A-Za-z0-9_]*)`?\s+(?:int|integer|bigint|smallint|tinyint|varchar|char|text|datetime|timestamp|date|decimal|double|float|boolean)\b",
                ),
            ],
        )
    }
}

impl LanguageAnalyzer for MyBatisXmlAnalyzer {
    fn language(&self) -> &'static str {
        "mybatis_xml"
    }

    fn analyze(&self, _path: &str, content: &str) -> CodeAnalysisResult {
        if content.contains('\0') {
            return CodeAnalysisResult {
                language: self.language().to_string(),
                analysis_level: "skipped".to_string(),
                parser_error: Some("binary_content".to_string()),
                symbols: Vec::new(),
            };
        }
        if content.trim().is_empty() {
            return CodeAnalysisResult {
                language: self.language().to_string(),
                analysis_level: "text_only".to_string(),
                parser_error: Some("empty_source".to_string()),
                symbols: Vec::new(),
            };
        }

        // MyBatis 动态 SQL 经常带有不完整片段、CDATA 和自定义实体；不依赖 XML DOM
        // 解析，避免一处格式问题让整份 Mapper 都无法检索。仅提取可验证的 DML 语句 id。
        let statement_pattern = Regex::new(r#"(?is)<\s*(select|insert|update|delete)\b([^>]*)>"#)
            .expect("MyBatis 语句标签正则必须有效");
        let id_pattern = Regex::new(r#"(?i)\bid\s*=\s*[\"']([^\"']+)[\"']"#)
            .expect("MyBatis 语句 ID 正则必须有效");
        let mut symbols = Vec::new();
        for statement in statement_pattern.captures_iter(content) {
            let Some(full_tag) = statement.get(0) else {
                continue;
            };
            if is_xml_ignored_region(content, full_tag.start()) {
                continue;
            }
            let Some(kind) = statement.get(1) else {
                continue;
            };
            let Some(attributes) = statement.get(2) else {
                continue;
            };
            let Some(id) = id_pattern
                .captures(attributes.as_str())
                .and_then(|item| item.get(1))
            else {
                continue;
            };
            let start_line = line_at(content, full_tag.start());
            symbols.push(AnalyzedCodeSymbol {
                kind: format!("mybatis_{}", kind.as_str().to_ascii_lowercase()),
                name: id.as_str().to_string(),
                qualified_name: id.as_str().to_string(),
                signature: full_tag.as_str().trim().to_string(),
                start_line,
                end_line: start_line,
            });
        }
        symbols.sort_by(|left, right| {
            (left.start_line, &left.name).cmp(&(right.start_line, &right.name))
        });
        symbols.dedup_by(|left, right| {
            left.kind == right.kind
                && left.name == right.name
                && left.start_line == right.start_line
        });
        CodeAnalysisResult {
            language: self.language().to_string(),
            analysis_level: if symbols.is_empty() {
                "text_only".to_string()
            } else {
                "structured_fallback".to_string()
            },
            parser_error: None,
            symbols,
        }
    }
}

fn is_xml_ignored_region(content: &str, byte_index: usize) -> bool {
    let prefix = &content[..byte_index];
    let comment_open = prefix.rfind("<!--");
    let comment_close = prefix.rfind("-->");
    if comment_open.is_some_and(|open| comment_close.is_none_or(|close| open > close)) {
        return true;
    }
    let cdata_open = prefix.rfind("<![CDATA[");
    let cdata_close = prefix.rfind("]]>");
    cdata_open.is_some_and(|open| cdata_close.is_none_or(|close| open > close))
}

fn analyze_with_patterns(
    language: &str,
    content: &str,
    patterns: &[(&str, &str)],
) -> CodeAnalysisResult {
    if content.contains('\0') {
        return CodeAnalysisResult {
            language: language.to_string(),
            analysis_level: "skipped".to_string(),
            parser_error: Some("binary_content".to_string()),
            symbols: Vec::new(),
        };
    }
    if content.trim().is_empty() {
        return CodeAnalysisResult {
            language: language.to_string(),
            analysis_level: "text_only".to_string(),
            parser_error: Some("empty_source".to_string()),
            symbols: Vec::new(),
        };
    }
    if unmatched_braces(content) {
        return CodeAnalysisResult {
            language: language.to_string(),
            analysis_level: "text_only".to_string(),
            parser_error: Some("unbalanced_delimiter".to_string()),
            symbols: Vec::new(),
        };
    }
    let mut symbols = Vec::new();
    for (kind, pattern) in patterns {
        let regex = Regex::new(pattern).expect("P0 源码分析正则必须有效");
        for captures in regex.captures_iter(content) {
            let Some(name) = captures.get(1) else {
                continue;
            };
            let start = line_at(content, name.start());
            let signature = content[name.start()..]
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            symbols.push(AnalyzedCodeSymbol {
                kind: (*kind).to_string(),
                name: name.as_str().to_string(),
                qualified_name: name.as_str().to_string(),
                signature,
                start_line: start,
                end_line: start,
            });
        }
    }
    symbols
        .sort_by(|left, right| (left.start_line, &left.name).cmp(&(right.start_line, &right.name)));
    symbols.dedup_by(|left, right| {
        left.kind == right.kind && left.name == right.name && left.start_line == right.start_line
    });
    CodeAnalysisResult {
        language: language.to_string(),
        analysis_level: if symbols.is_empty() {
            "text_only".to_string()
        } else {
            "structured_fallback".to_string()
        },
        parser_error: None,
        symbols,
    }
}

fn line_at(content: &str, byte_index: usize) -> i64 {
    i64::try_from(
        content[..byte_index]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
    )
    .unwrap_or(i64::MAX)
}

fn unmatched_braces(content: &str) -> bool {
    let mut depth = 0_i64;
    for character in content.chars() {
        match character {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    depth != 0
}

#[cfg(test)]
mod tests {
    use super::P0LanguageAnalyzer;

    #[test]
    fn analyzes_all_p0_languages_with_visible_quality() {
        for (path, source, expected) in [
            ("src/lib.rs", "pub struct App {}\npub fn run() {}", "App"),
            (
                "src/api.ts",
                "export interface Api {}\nexport function call() {}",
                "Api",
            ),
            (
                "src/App.vue",
                "<script setup lang=\"ts\">\nconst state = 1\n</script>",
                "App",
            ),
            (
                "src/App.java",
                "public class App { public void run() {} }",
                "App",
            ),
            ("schema.sql", "CREATE TABLE orders (id INTEGER);", "orders"),
            (
                "mapper/OrderMapper.xml",
                "<mapper><select id=\"findOpenOrders\">SELECT 1</select></mapper>",
                "findOpenOrders",
            ),
        ] {
            let result = P0LanguageAnalyzer::analyze_path(path, source);
            assert_eq!(result.analysis_level, "structured_fallback");
            assert!(result.symbols.iter().any(|symbol| symbol.name == expected));
        }
    }

    #[test]
    fn reports_safe_fallback_levels_for_invalid_or_unknown_content() {
        assert_eq!(
            P0LanguageAnalyzer::analyze_path("bad.rs", "fn bad() {").analysis_level,
            "text_only"
        );
        assert_eq!(
            P0LanguageAnalyzer::analyze_path("binary.rs", "a\0b").analysis_level,
            "skipped"
        );
        assert_eq!(
            P0LanguageAnalyzer::analyze_path("README.md", "# x").analysis_level,
            "structured_fallback"
        );
        assert!(P0LanguageAnalyzer::analyze_path("README.md", "# x")
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "heading" && symbol.name == "x"));
    }

    #[test]
    fn extracts_p0_framework_symbols_without_claiming_ast_precision() {
        let rust = P0LanguageAnalyzer::analyze_path(
            "src/commands.rs",
            "#[tauri::command]\npub fn save() {}\n#[derive(Serialize)]\npub struct Request {}\n#[test]\nfn saves() {}\nlet _ = std::env::var(\"MODE\");",
        );
        assert!(rust
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "command" && symbol.name == "save"));
        assert!(rust
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "model" && symbol.name == "Request"));
        assert!(rust
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "test" && symbol.name == "saves"));
        assert!(rust
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "config_key" && symbol.name == "MODE"));

        let typescript = P0LanguageAnalyzer::analyze_path(
            "src/api.ts",
            "export function load() { invoke(\"save\"); get(\"/api/orders\"); localStorage.getItem(\"theme\"); test(\"loads\", () => {}); }",
        );
        for (kind, name) in [
            ("ipc_command", "save"),
            ("route", "/api/orders"),
            ("config_key", "theme"),
            ("test", "loads"),
        ] {
            assert!(typescript
                .symbols
                .iter()
                .any(|symbol| symbol.kind == kind && symbol.name == name));
        }

        let java = P0LanguageAnalyzer::analyze_path(
            "OrderController.java",
            "@FeignClient(name = \"order-provider\")\ninterface Orders {}\n@GetMapping(\"/orders\")\npublic class OrderController { @Test public void reads() {} }",
        );
        assert!(java
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "feign_client"));
        assert!(java.symbols.iter().any(|symbol| symbol.kind == "route"));

        let sql = P0LanguageAnalyzer::analyze_path(
            "orders.sql",
            "CREATE TABLE orders (\n id BIGINT,\n status VARCHAR(32)\n);",
        );
        assert!(sql
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "table" && symbol.name == "orders"));
        assert!(sql
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "column" && symbol.name == "id"));
    }

    #[test]
    fn extracts_mybatis_statement_ids_without_requiring_well_formed_xml() {
        let mapper = P0LanguageAnalyzer::analyze_path(
            "mapper/BdaWorkOrderDetailMapper.xml",
            r#"<mapper>
                <!-- <select id="commentedOut">SELECT 0</select> -->
                <select resultMap="BaseResultMap" id="selectCandidateWorkOrdersByUserIds">
                    SELECT * FROM orders WHERE receive_uid IN <foreach>#{uid}</foreach>
                </select>
                <update id='markHandled'>UPDATE orders SET state_id = 1</update>
                <select id="broken">SELECT <![CDATA[ a < b ]]>
            </mapper>"#,
        );
        assert_eq!(mapper.language, "mybatis_xml");
        assert_eq!(mapper.analysis_level, "structured_fallback");
        assert!(mapper.symbols.iter().any(|symbol| {
            symbol.kind == "mybatis_select"
                && symbol.name == "selectCandidateWorkOrdersByUserIds"
                && symbol.start_line == 3
        }));
        assert!(!mapper
            .symbols
            .iter()
            .any(|symbol| symbol.name == "commentedOut"));
        assert!(mapper
            .symbols
            .iter()
            .any(|symbol| symbol.kind == "mybatis_update" && symbol.name == "markHandled"));
    }
}
