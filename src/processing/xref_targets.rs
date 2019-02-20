use actix::{Actor, Addr, Handler, Message, SyncArbiter, SyncContext};
use diesel::{
    Connection as _Connection,
    prelude::*,
};
use failure::Error;
use minidom::{Element, Node};
use std::{
    collections::HashMap,
    str::FromStr,
};

use crate::{
    db::{
        Connection,
        Pool,
        models as db,
        schema::{documents, modules, xref_targets},
    },
    models::File,
};

const CNXML_NS: &str = "http://cnx.rice.edu/cnxml";

/// Process a document to create list of cross-reference targets within it.
///
/// This function will insert new records into database, but will do so without
/// a transaction. You'll probably want to wrap it in one.
pub fn process_document(dbconn: &Connection, document: &db::Document)
    -> Result<(), Error>
{
    let index = File::by_id(dbconn, document.index)?;
    let content = index.read_to_string()?;
    let root = Element::from_str(&content)?;

    let mut last_context = None;
    let mut counters = HashMap::with_capacity(8);

    for element in iter_tree(&root) {
        let id = match element.attr("id") {
            Some(id) => id,
            None => continue,
        };

        let description = match element.name() {
            // Elements that hold block leafs, or are themselves block leafs
            "example" | "solution" | "commentary" | "note"
                => line_contex_text(element),
            // Elements that have captions
            "figure" | "subfigure"
                => element.get_child("caption", CNXML_NS)
                    .and_then(line_contex_text),
            // Description of an exercise is the same as of its problem, as we
            // consider references to both to be the same.
            "exercise" => element.get_child("problem", CNXML_NS)
                .and_then(line_contex_text),
            // We don't support making references to other types.
            _ => continue,
        };

        let type_ = match element.name() {
            "note" => element.attr("type")
                .unwrap_or("note"),
            "subfigure" => "figure",
            name => name,
        };

        let context = match element.name() {
            // Targets which can own other targets set context.
            "exercise" | "figure" => {
                last_context = Some(id);
                None
            }
            // Targets owned by them use that context.
            "problem" | "solution" | "commentary" | "subfigure" => last_context,
            // Other elements have no context.
            _ => {
                last_context = None;
                None
            }
        };

        // Reset scoped counters.
        match element.name() {
            "exercise" => {
                counters.insert("solution", 0);
            }
            "figure" => {
                counters.insert("subfigure", 0);
            }
            _ => {}
        }

        let counter = *counters.entry(element.name())
            .and_modify(|x| *x += 1)
            .or_insert(1);

        let target = db::NewXrefTarget {
            document: document.id,
            element: id,
            type_,
            description: description.as_ref().map(String::as_str),
            context,
            counter,
        };

        diesel::insert_into(xref_targets::table)
            .values(&target)
            .on_conflict((xref_targets::document, xref_targets::element))
            .do_update()
            .set(&target)
            .execute(dbconn)?;
    }

    diesel::update(document)
        .set(documents::xrefs_ready.eq(true))
        .execute(dbconn)?;

    Ok(())
}

/// Process all documents which don't yet have a cross-reference target list
/// created.
///
/// Normally documents have their lists created or updated just after they are
/// modified, but in some cases it might be possible that the server exited
/// before their generation was completed. This function will be called each
/// time the server starts to process such documents.
pub fn process_stale(dbconn: &Connection) -> Result<(), Error> {
    dbconn.transaction(|| {
        let documents = modules::table
            .inner_join(documents::table)
            .filter(documents::xrefs_ready.eq(false))
            .get_results::<(db::Module, db::Document)>(dbconn)?;

        for (_, document) in documents {
            process_document(dbconn, &document)?;
        }

        Ok(())
    })
}

/// DFS over a DOM element.
fn iter_tree(el: &Element) -> impl Iterator<Item = &Element> {
    std::iter::once(el)
        .chain(IterTree(vec![el.children()]))
}

struct IterTree<'s>(Vec<minidom::Children<'s>>);

impl<'s> Iterator for IterTree<'s> {
    type Item = &'s Element;

    fn next(&mut self) -> Option<&'s Element> {
        while let Some(mut el) = self.0.pop() {
            if let Some(next) = el.next() {
                self.0.push(el);
                self.0.push(next.children());
                return Some(next);
            }
        }

        None
    }
}

/// Get raw text content of the first line context within an element.
fn line_contex_text(e: &Element) -> Option<String> {
    match e.name() {
        // Leaf blocks and line context
        "para" | "title" | "item" | "caption" | "emphasis" | "sub" | "sup"
        | "link" => {
            let mut r = String::new();

            for node in e.nodes() {
                match node {
                    Node::Text(ref t) => r.extend(collapse_spaces(t)),
                    Node::Element(ref e) => if let Some(s) = line_contex_text(e) {
                        r.extend(collapse_spaces(&s));
                    },
                    Node::Comment(_) => (),
                }
            }

            if let Some((len, _)) = r.char_indices().skip(240).next() {
                r.truncate(len);
            }

            if r.is_empty() {
                None
            } else {
                Some(r)
            }
        }
        // Elements which we know don't have any sensible textual representation.
        "math" | "media" | "image" => None,
        // Compound blocks and other nodes.
        _ => e.children()
            .next()
            .and_then(line_contex_text),
    }
}

fn collapse_spaces(s: &str) -> impl Iterator<Item = &str> {
    let mut items = s.split_whitespace();

    items.next().into_iter()
        .chain(items.map(|i| Pair::Pair(" ", i)).flatten())
}

enum Pair<T> {
    Pair(T, T),
    One(T),
    Empty,
}

impl<T> Iterator for Pair<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match std::mem::replace(self, Pair::Empty) {
            Pair::Pair(a, b) => {
                *self = Pair::One(b);
                Some(a)
            }
            Pair::One(b) => {
                *self = Pair::Empty;
                Some(b)
            }
            Pair::Empty => None,
        }
    }
}

pub struct ProcessDocument {
    pub document: db::Document,
}

/// Request a cross-reference target list to be generated for a document.
impl Message for ProcessDocument {
    type Result = ();
}

/// Actix actor handling generation of cross-reference target lists for newly
/// uploaded documents.
pub struct TargetProcessor {
    pool: Pool,
}

impl TargetProcessor {
    pub fn new(pool: Pool) -> TargetProcessor {
        TargetProcessor {
            pool,
        }
    }

    pub fn start(pool: Pool) -> Addr<TargetProcessor> {
        SyncArbiter::start(1, move || TargetProcessor::new(pool.clone()))
    }

    fn process(&mut self, document: &db::Document) -> Result<(), Error> {
        let db = self.pool.get()?;
        process_document(&*db, document)?;
        Ok(())
    }

    fn process_stale(&mut self) -> Result<(), Error> {
        let db = self.pool.get()?;
        process_stale(&*db)?;
        Ok(())
    }
}

impl Actor for TargetProcessor {
    type Context = SyncContext<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        if let Err(err) = self.process_stale() {
            error!("Could not process stale documents: {}", err);
        }
    }
}

impl Handler<ProcessDocument> for TargetProcessor {
    type Result = ();

    fn handle(&mut self, msg: ProcessDocument, _: &mut Self::Context) {
        if let Err(err) = self.process(&msg.document) {
            error!("Could not process xrefs for document {}: {}",
                msg.document.id, err);
        }
    }
}
