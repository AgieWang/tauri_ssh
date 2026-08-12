use std::cmp::Ordering;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ammonia::Builder as AmmoniaBuilder;
use calamine::{open_workbook_auto, Reader};
use fastembed::{
    EmbeddingModel, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rusqlite::{params, Connection};
use scraper::{Html, Selector};
use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

type SpikeResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Serialize)]
struct FtsCase {
    query: String,
    unicode61_hits: Vec<String>,
    trigram_hits: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FtsReport {
    sqlite_version: String,
    compile_options: Vec<String>,
    trigram_available: bool,
    cases: Vec<FtsCase>,
}

#[derive(Debug, Serialize)]
struct RetrievalMetric {
    query: String,
    expected: String,
    first_hit: String,
    reciprocal_rank: f32,
}

#[derive(Debug, Serialize)]
struct EmbeddingReport {
    model: String,
    dimension: usize,
    initialization_ms: u128,
    embedding_ms: u128,
    recall_at_1: f32,
    mrr: f32,
    metrics: Vec<RetrievalMetric>,
}

#[derive(Debug, Serialize)]
struct VectorScaleReport {
    chunk_count: usize,
    dimension: usize,
    database_bytes: u64,
    build_ms: u128,
    query_p50_ms: f64,
    query_p95_ms: f64,
    query_samples_ms: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct HtmlSanitizationReport {
    removed_script: bool,
    removed_event_handler: bool,
    removed_external_resource: bool,
    preserved_visible_text: bool,
    extracted_text: String,
    sanitized_length: usize,
    large_dom_completed: bool,
    large_dom_length: usize,
}

#[derive(Debug, Serialize)]
struct OfficeExtractionReport {
    docx_text: String,
    xlsx_text: String,
    pptx_text: String,
    pdf_text: String,
    damaged_file_rejected: bool,
    elapsed_ms: u128,
}

fn main() -> SpikeResult<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    match command.as_str() {
        "fts" => run_fts(),
        "embedding" => {
            let model = args.next().unwrap_or_else(|| "all".to_string());
            let model_dir = args
                .next()
                .ok_or("embedding Spike 必须提供经校验的离线模型包目录")?;
            run_embedding(&model, Path::new(&model_dir))
        }
        "vector" => {
            let count = args
                .next()
                .unwrap_or_else(|| "100000".to_string())
                .parse::<usize>()?;
            let dimension = args
                .next()
                .unwrap_or_else(|| "384".to_string())
                .parse::<usize>()?;
            run_vector(count, dimension)
        }
        "html" => run_html_sanitization(),
        "office" => run_office_extraction(),
        _ => {
            eprintln!(
                "usage:\n  cargo run --release -- fts\n  \
                 cargo run --release -- embedding [e5|bge] <offline_model_dir>\n  \
                 cargo run --release -- vector [chunk_count] [dimension]\n  \
                 cargo run --release -- html\n  \
                 cargo run --release -- office"
            );
            Ok(())
        }
    }
}

fn run_office_extraction() -> SpikeResult<()> {
    let started = Instant::now();
    let temp_root = env::temp_dir().join(format!(
        "knowledge-office-spike-{}",
        std::process::id()
    ));
    if temp_root.exists() {
        fs::remove_dir_all(&temp_root)?;
    }
    fs::create_dir_all(&temp_root)?;
    let docx_path = temp_root.join("sample.docx");
    let xlsx_path = temp_root.join("sample.xlsx");
    let pptx_path = temp_root.join("sample.pptx");
    let broken_path = temp_root.join("broken.docx");

    write_zip(
        &docx_path,
        &[(
            "word/document.xml",
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>中文标题</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>表格单元格</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        )],
    )?;
    write_zip(
        &pptx_path,
        &[(
            "ppt/slides/slide1.xml",
            r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><a:t>第一页内容</a:t></p:spTree></p:cSld></p:sld>"#,
        )],
    )?;
    write_zip(
        &xlsx_path,
        &[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="需求" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>需求编号</t></is></c><c r="B1" t="inlineStr"><is><t>REQ-1042</t></is></c></row></sheetData></worksheet>"#,
            ),
        ],
    )?;
    fs::write(&broken_path, b"not a zip")?;

    let docx_text = read_ooxml_text(&docx_path, "word/document.xml")?;
    let pptx_text = read_ooxml_text(&pptx_path, "ppt/slides/slide1.xml")?;
    let mut workbook = open_workbook_auto(&xlsx_path)?;
    let sheet_name = workbook.sheet_names().first().cloned().ok_or("XLSX 缺少工作表")?;
    let xlsx_text = workbook
        .worksheet_range(&sheet_name)?
        .rows()
        .flatten()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" | ");
    let pdf_text = pdf_extract::extract_text_from_mem(&minimal_pdf("PDF fixture"))?;
    let damaged_file_rejected = read_ooxml_text(&broken_path, "word/document.xml").is_err();
    fs::remove_dir_all(&temp_root)?;

    print_json(&OfficeExtractionReport {
        docx_text,
        xlsx_text,
        pptx_text,
        pdf_text,
        damaged_file_rejected,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn write_zip(path: &Path, entries: &[(&str, &str)]) -> SpikeResult<()> {
    let mut archive = ZipWriter::new(fs::File::create(path)?);
    for (name, content) in entries {
        archive.start_file(name, SimpleFileOptions::default())?;
        archive.write_all(content.as_bytes())?;
    }
    archive.finish()?;
    Ok(())
}

fn read_ooxml_text(path: &Path, entry_name: &str) -> SpikeResult<String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut entry = archive.by_name(entry_name)?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut entry, &mut xml)?;
    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut text = Vec::new();
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            quick_xml::events::Event::Text(value) => text.push(value.decode()?.into_owned()),
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(text.join(" "))
}

fn minimal_pdf(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 12 Tf 20 120 Td ({text}) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for offset in offsets.iter().skip(1) {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    pdf.into_bytes()
}

fn run_html_sanitization() -> SpikeResult<()> {
    let unsafe_html = r#"
        <main><h1>中文原型</h1><p>订单审批 <code>POST /api/orders</code></p>
        <script>window.__exfiltrate = 'secret';</script>
        <img src="https://untrusted.example/image.png" onerror="alert('xss')">
        <a href="javascript:alert('xss')">危险链接</a>
        <svg><script>alert('svg')</script><circle onload="alert('event')" /></svg>
    "#;
    let sanitized = AmmoniaBuilder::default().clean(unsafe_html).to_string();
    let large_dom = format!("<main>{}</main>", "<p>节点</p>".repeat(20_000));
    let sanitized_large_dom = AmmoniaBuilder::default().clean(&large_dom).to_string();
    let document = Html::parse_document(&sanitized);
    let main_text = Selector::parse("main, body")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|element| element.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));

    print_json(&HtmlSanitizationReport {
        removed_script: !sanitized.to_ascii_lowercase().contains("<script"),
        removed_event_handler: !sanitized.to_ascii_lowercase().contains("onerror")
            && !sanitized.to_ascii_lowercase().contains("onload"),
        removed_external_resource: !sanitized.contains("untrusted.example"),
        preserved_visible_text: main_text.contains("中文原型") && main_text.contains("订单审批"),
        extracted_text: main_text,
        sanitized_length: sanitized.len(),
        large_dom_completed: sanitized_large_dom.contains("节点"),
        large_dom_length: sanitized_large_dom.len(),
    })
}

fn run_fts() -> SpikeResult<()> {
    let conn = Connection::open_in_memory()?;
    let sqlite_version = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let compile_options = conn
        .prepare("PRAGMA compile_options")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE docs_unicode USING fts5(key UNINDEXED, body, tokenize='unicode61');
        INSERT INTO docs_unicode(key, body) VALUES
          ('requirement', '支付项目 v2.3.1 版本需要新增退款审批需求 REQ-1042'),
          ('rust', 'Rust Command get_order_detail 调用 OrderService 查询订单'),
          ('java', 'Java com.example.order.OrderService 通过 FeignClient 调用 /api/orders/{id}'),
          ('sql', 'SQL 表 ads_work_order 的字段 warning_time 需要建立索引'),
          ('path', '前端文件 src/pages/knowledge/index.tsx 负责知识库检索');
        ",
    )?;

    let trigram_available = conn
        .execute_batch(
            "
            CREATE VIRTUAL TABLE docs_trigram USING fts5(key UNINDEXED, body, tokenize='trigram');
            INSERT INTO docs_trigram SELECT key, body FROM docs_unicode;
            ",
        )
        .is_ok();

    let queries = [
        "退款审批",
        "REQ-1042",
        "v2.3.1",
        "get_order_detail",
        "OrderService",
        "/api/orders",
        "warning_time",
        "src/pages/knowledge",
    ];
    let cases = queries
        .iter()
        .map(|query| {
            let unicode61_hits = fts_hits(&conn, "docs_unicode", query)?;
            let trigram_hits = if trigram_available {
                fts_hits(&conn, "docs_trigram", query)?
            } else {
                Vec::new()
            };
            Ok(FtsCase {
                query: (*query).to_string(),
                unicode61_hits,
                trigram_hits,
            })
        })
        .collect::<SpikeResult<Vec<_>>>()?;

    print_json(&FtsReport {
        sqlite_version,
        compile_options,
        trigram_available,
        cases,
    })
}

fn fts_hits(conn: &Connection, table: &str, query: &str) -> SpikeResult<Vec<String>> {
    let sql =
        format!("SELECT key FROM {table} WHERE {table} MATCH ?1 ORDER BY bm25({table}) LIMIT 5");
    let escaped_query = format!("\"{}\"", query.replace('"', "\"\""));
    let hits = conn
        .prepare(&sql)?
        .query_map([escaped_query], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(hits)
}

fn run_embedding(requested: &str, model_dir: &Path) -> SpikeResult<()> {
    let candidates = [
        ("e5", EmbeddingModel::MultilingualE5Small),
        ("bge", EmbeddingModel::BGESmallZHV15),
    ];
    let mut reports = Vec::new();
    for (key, model) in candidates {
        if requested != "all" && requested != key {
            continue;
        }
        reports.push(benchmark_model(key, model, model_dir)?);
    }
    print_json(&reports)
}

fn benchmark_model(
    key: &str,
    _model: EmbeddingModel,
    model_dir: &Path,
) -> SpikeResult<EmbeddingReport> {
    let documents = [
        (
            "pay-v231",
            "支付项目 v2.3.1 版本实现退款审批需求 REQ-1042。",
        ),
        ("pay-v240", "支付项目 v2.4.0 将退款审批改为自动风控审核。"),
        (
            "order-command",
            "Rust Command get_order_detail 调用 OrderService。",
        ),
        (
            "order-api",
            "Java FeignClient 调用 /api/orders/{id} 查询订单。",
        ),
        (
            "warning-sql",
            "SQL 表 ads_work_order 的 warning_time 字段用于预警。",
        ),
        (
            "knowledge-ui",
            "React 页面 src/pages/knowledge/index.tsx 展示知识库检索。",
        ),
        ("zentao-test", "禅道测试单记录 REQ-1042 已通过回归测试。"),
        (
            "deployment",
            "Jenkins deployment job 发布 v2.3.1 到生产环境。",
        ),
        ("ssh", "SSH 终端支持跳板机连接与命令审计。"),
        ("inventory", "库存项目增加仓库盘点与差异复核流程。"),
        ("customer", "客户项目新增客户等级和联系人维护。"),
        ("logging", "日志系统使用 trace_id 关联前后端请求。"),
    ];
    let queries = [
        ("支付项目 2.3.1 的退款需求是什么", "pay-v231"),
        ("get_order_detail 由哪个服务实现", "order-command"),
        ("订单查询的 HTTP API 路径", "order-api"),
        ("预警时间对应哪个表字段", "warning-sql"),
        ("知识库 React 页面代码路径", "knowledge-ui"),
        ("REQ-1042 的测试结果", "zentao-test"),
        ("which release was deployed by Jenkins", "deployment"),
        ("跨项目查找仓库盘点方案", "inventory"),
    ];

    let started = Instant::now();
    let mut embedding = load_offline_embedding(key, model_dir)?;
    let initialization_ms = started.elapsed().as_millis();

    let document_inputs = documents
        .iter()
        .map(|(_, text)| {
            if key == "e5" {
                format!("passage: {text}")
            } else {
                (*text).to_string()
            }
        })
        .collect::<Vec<_>>();
    let query_inputs = queries
        .iter()
        .map(|(query, _)| {
            if key == "e5" {
                format!("query: {query}")
            } else {
                format!("为这个句子生成表示以用于检索相关文章：{query}")
            }
        })
        .collect::<Vec<_>>();

    let embedding_started = Instant::now();
    let document_vectors = embedding.embed(document_inputs, Some(16))?;
    let query_vectors = embedding.embed(query_inputs, Some(16))?;
    let embedding_ms = embedding_started.elapsed().as_millis();
    let dimension = document_vectors.first().map_or(0, Vec::len);

    let mut metrics = Vec::new();
    for ((query, expected), query_vector) in queries.iter().zip(query_vectors.iter()) {
        let mut ranked = document_vectors
            .iter()
            .enumerate()
            .map(|(index, document_vector)| (index, dot(query_vector, document_vector)))
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let rank = ranked
            .iter()
            .position(|(index, _)| documents[*index].0 == *expected)
            .map(|index| index + 1)
            .unwrap_or(documents.len());
        metrics.push(RetrievalMetric {
            query: (*query).to_string(),
            expected: (*expected).to_string(),
            first_hit: documents[ranked[0].0].0.to_string(),
            reciprocal_rank: 1.0 / rank as f32,
        });
    }

    let recall_at_1 = metrics
        .iter()
        .filter(|metric| metric.first_hit == metric.expected)
        .count() as f32
        / metrics.len() as f32;
    let mrr = metrics
        .iter()
        .map(|metric| metric.reciprocal_rank)
        .sum::<f32>()
        / metrics.len() as f32;

    Ok(EmbeddingReport {
        model: key.to_string(),
        dimension,
        initialization_ms,
        embedding_ms,
        recall_at_1,
        mrr,
        metrics,
    })
}

/// 评测包必须完整携带模型、Tokenizer 与当前平台 ONNX Runtime；脚本不会下载或
/// 回退到其他模型，因此输出可关联到稳定的本地目录摘要。
fn load_offline_embedding(key: &str, model_dir: &Path) -> SpikeResult<TextEmbedding> {
    let model_dir = fs::canonicalize(model_dir)?;
    let runtime = model_dir
        .join("runtime")
        .join(if cfg!(target_os = "windows") {
            "onnxruntime.dll"
        } else if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        });
    if !runtime.is_file() {
        return Err(
            "offline model package does not include the current-platform ONNX Runtime".into(),
        );
    }
    std::env::set_var("ORT_DYLIB_PATH", runtime);
    let read = |name: &str| -> SpikeResult<Vec<u8>> { Ok(fs::read(model_dir.join(name))?) };
    let tokenizer = TokenizerFiles {
        tokenizer_file: read("tokenizer.json")?,
        config_file: read("config.json")?,
        special_tokens_map_file: read("special_tokens_map.json")?,
        tokenizer_config_file: read("tokenizer_config.json")?,
    };
    let pooling = if key == "bge" {
        Pooling::Cls
    } else {
        Pooling::Mean
    };
    let model =
        UserDefinedEmbeddingModel::new(read("model.onnx")?, tokenizer).with_pooling(pooling);
    Ok(TextEmbedding::try_new_from_user_defined(
        model,
        Default::default(),
    )?)
}

fn run_vector(chunk_count: usize, dimension: usize) -> SpikeResult<()> {
    let database_path = benchmark_database_path(chunk_count, dimension);
    if database_path.exists() {
        fs::remove_file(&database_path)?;
    }
    let mut conn = Connection::open(&database_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(
        "
        CREATE TABLE vectors (
            id INTEGER PRIMARY KEY,
            project_id INTEGER NOT NULL,
            release_id INTEGER NOT NULL,
            dimension INTEGER NOT NULL,
            vector BLOB NOT NULL
        );
        CREATE INDEX idx_vectors_scope ON vectors(project_id, release_id);
        ",
    )?;

    let build_started = Instant::now();
    let mut rng = StdRng::seed_from_u64(20260730);
    {
        let transaction = conn.transaction()?;
        let mut statement = transaction.prepare(
            "INSERT INTO vectors(id, project_id, release_id, dimension, vector)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for id in 0..chunk_count {
            let vector = random_normalized_vector(&mut rng, dimension);
            statement.execute(params![
                id as i64,
                (id % 20) as i64,
                (id % 10) as i64,
                dimension as i64,
                encode_vector(&vector)
            ])?;
        }
        drop(statement);
        transaction.commit()?;
    }
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let build_ms = build_started.elapsed().as_millis();

    let query = random_normalized_vector(&mut rng, dimension);
    let mut samples = Vec::new();
    for _ in 0..12 {
        let started = Instant::now();
        let mut best = Vec::<(i64, f32)>::with_capacity(10);
        let mut statement = conn.prepare(
            "SELECT id, vector FROM vectors
             WHERE project_id IN (0, 1, 2, 3, 4)
             ORDER BY id",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id = row.get::<_, i64>(0)?;
            let blob = row.get::<_, Vec<u8>>(1)?;
            let score = dot_blob(&query, &blob)?;
            push_top_k(&mut best, (id, score), 10);
        }
        if best.is_empty() {
            return Err("vector benchmark returned no candidates".into());
        }
        samples.push(started.elapsed());
    }

    let database_bytes = fs::metadata(&database_path)?.len();
    let mut milliseconds = samples.iter().skip(2).map(duration_ms).collect::<Vec<_>>();
    milliseconds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let report = VectorScaleReport {
        chunk_count,
        dimension,
        database_bytes,
        build_ms,
        query_p50_ms: percentile(&milliseconds, 0.50),
        query_p95_ms: percentile(&milliseconds, 0.95),
        query_samples_ms: milliseconds,
    };
    print_json(&report)
}

fn benchmark_database_path(chunk_count: usize, dimension: usize) -> PathBuf {
    env::temp_dir().join(format!(
        "tauri-ssh-knowledge-vector-{chunk_count}-{dimension}.sqlite"
    ))
}

fn random_normalized_vector(rng: &mut StdRng, dimension: usize) -> Vec<f32> {
    let mut vector = (0..dimension)
        .map(|_| rng.gen_range(-1.0_f32..1.0_f32))
        .collect::<Vec<_>>();
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    for value in &mut vector {
        *value /= norm;
    }
    vector
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn dot_blob(query: &[f32], blob: &[u8]) -> SpikeResult<f32> {
    if blob.len() != query.len() * std::mem::size_of::<f32>() {
        return Err(format!(
            "vector blob size mismatch: expected {}, got {}",
            query.len() * std::mem::size_of::<f32>(),
            blob.len()
        )
        .into());
    }
    let score = query
        .iter()
        .zip(blob.chunks_exact(4))
        .map(|(left, bytes)| {
            let right = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            left * right
        })
        .sum();
    Ok(score)
}

fn push_top_k(items: &mut Vec<(i64, f32)>, candidate: (i64, f32), limit: usize) {
    items.push(candidate);
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    items.truncate(limit);
}

fn percentile(samples: &[f64], percentile: f64) -> f64 {
    let index = ((samples.len() - 1) as f64 * percentile).ceil() as usize;
    samples[index]
}

fn duration_ms(duration: &Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn print_json<T: Serialize>(value: &T) -> SpikeResult<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[allow(dead_code)]
fn ensure_parent(path: &Path) -> SpikeResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
