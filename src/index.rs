use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, TantivyDocument};

/// Metadata collected during conversion for indexing
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub body: String,
    pub date: Option<String>,
    pub year: Option<String>,
    pub month: Option<String>,
    pub message_count: u64,
    pub file_path: String,
}

/// Search result returned by search operations
#[derive(Debug, serde::Serialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub date: String,
    pub year: String,
    pub month: String,
    pub message_count: u64,
    pub file_path: String,
    pub score: f32,
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT); // not stored — saves space
    builder.add_text_field("date", STRING | STORED);
    builder.add_text_field("year", STRING | STORED);
    builder.add_text_field("month", STRING | STORED);
    builder.add_u64_field("message_count", STORED);
    builder.add_text_field("file_path", STRING | STORED);
    builder.build()
}

/// Build a Tantivy index from conversation metadata.
/// Returns the number of indexed documents.
pub fn build_index(index_path: &Path, metas: &[ConversationMeta]) -> Result<usize, Box<dyn std::error::Error>> {
    let schema = build_schema();

    // Create or overwrite index directory
    if index_path.exists() {
        std::fs::remove_dir_all(index_path)?;
    }
    std::fs::create_dir_all(index_path)?;

    let index = Index::create_in_dir(index_path, schema.clone())?;
    let mut writer: IndexWriter = index.writer(50_000_000)?; // 50MB buffer

    let id_field = schema.get_field("id").unwrap();
    let title_field = schema.get_field("title").unwrap();
    let body_field = schema.get_field("body").unwrap();
    let date_field = schema.get_field("date").unwrap();
    let year_field = schema.get_field("year").unwrap();
    let month_field = schema.get_field("month").unwrap();
    let msg_count_field = schema.get_field("message_count").unwrap();
    let file_path_field = schema.get_field("file_path").unwrap();

    for meta in metas {
        writer.add_document(doc!(
            id_field => meta.id.as_str(),
            title_field => meta.title.as_str(),
            body_field => meta.body.as_str(),
            date_field => meta.date.as_deref().unwrap_or(""),
            year_field => meta.year.as_deref().unwrap_or(""),
            month_field => meta.month.as_deref().unwrap_or(""),
            msg_count_field => meta.message_count,
            file_path_field => meta.file_path.as_str(),
        ))?;
    }

    writer.commit()?;
    Ok(metas.len())
}

/// Wrapper around a Tantivy index for search operations
pub struct SearchIndex {
    index: Index,
    schema: Schema,
}

impl SearchIndex {
    /// Open an existing index from disk
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let index = Index::open_in_dir(path)?;
        let schema = index.schema();
        Ok(SearchIndex { index, schema })
    }

    /// Full-text search across title and body fields
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let reader = self.index.reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();

        let title_field = self.schema.get_field("title").unwrap();
        let body_field = self.schema.get_field("body").unwrap();

        let query_parser = QueryParser::for_index(&self.index, vec![title_field, body_field]);
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(self.doc_to_result(&doc, score));
        }

        Ok(results)
    }

    /// Get a single conversation by ID
    pub fn get_by_id(&self, id: &str) -> Result<Option<SearchResult>, Box<dyn std::error::Error>> {
        let reader = self.index.reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();

        let id_field = self.schema.get_field("id").unwrap();
        let term = tantivy::Term::from_field_text(id_field, id);
        let query = TermQuery::new(term, IndexRecordOption::Basic);

        let top_docs = searcher.search(&query, &TopDocs::with_limit(1))?;

        if let Some((score, doc_address)) = top_docs.into_iter().next() {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            Ok(Some(self.doc_to_result(&doc, score)))
        } else {
            Ok(None)
        }
    }

    /// List conversations filtered by year and/or month
    pub fn list_by_date(
        &self,
        year: Option<&str>,
        month: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let reader = self.index.reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();

        let mut clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        if let Some(y) = year {
            let year_field = self.schema.get_field("year").unwrap();
            let term = tantivy::Term::from_field_text(year_field, y);
            clauses.push((Occur::Must, Box::new(TermQuery::new(term, IndexRecordOption::Basic))));
        }

        if let Some(m) = month {
            let month_field = self.schema.get_field("month").unwrap();
            let term = tantivy::Term::from_field_text(month_field, m);
            clauses.push((Occur::Must, Box::new(TermQuery::new(term, IndexRecordOption::Basic))));
        }

        let query: Box<dyn tantivy::query::Query> = if clauses.is_empty() {
            Box::new(AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        let top_docs = searcher.search(&*query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(self.doc_to_result(&doc, score));
        }

        Ok(results)
    }

    fn doc_to_result(&self, doc: &TantivyDocument, score: f32) -> SearchResult {
        let get_text = |field_name: &str| -> String {
            let field = self.schema.get_field(field_name).unwrap();
            doc.get_first(field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };

        let msg_count_field = self.schema.get_field("message_count").unwrap();
        let message_count = doc
            .get_first(msg_count_field)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        SearchResult {
            id: get_text("id"),
            title: get_text("title"),
            date: get_text("date"),
            year: get_text("year"),
            month: get_text("month"),
            message_count,
            file_path: get_text("file_path"),
            score,
        }
    }
}
