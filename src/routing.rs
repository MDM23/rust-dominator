use std::cell::RefCell;
use std::collections::HashMap;

use futures_signals::signal::{Mutable, Signal, SignalExt};
use once_cell::sync::Lazy;
use wasm_bindgen::JsValue;

use crate::{bindings, Dom};

thread_local! {
    static ROUTER: Lazy<Router> = Lazy::new(|| Router::new(&bindings::current_pathname()));
}

pub struct Router {
    current_path: Mutable<Vec<String>>,
    remainder: RefCell<Vec<String>>,
}

impl Router {
    fn new(path: &str) -> Self {
        let segments = split_path(path);

        Self {
            current_path: Mutable::new(segments.clone()),
            remainder: RefCell::new(segments),
        }
    }

    pub fn signal_path() -> impl Signal<Item = Vec<String>> {
        ROUTER
            .with(|r| r.current_path.signal_cloned())
            .map(|_| ROUTER.with(|r| r.remainder.borrow().clone()))
    }

    pub fn set_remainder(remainder: Vec<String>) {
        ROUTER.with(|r| r.remainder.replace(remainder));
    }

    pub fn goto(path: &str) {
        ROUTER.with(|r| {
            let segments = split_path(path);

            r.remainder.replace(segments.clone());
            r.current_path.replace(segments);

            web_sys::window()
                .unwrap()
                .history()
                .unwrap()
                .push_state_with_url(&JsValue::NULL, "", Some(path))
                .unwrap();
        });
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Segment {
    Static(String),
    Param(String),
    Continue,
}

#[derive(Debug, Clone)]
pub struct RouteMatch {
    path: Vec<String>,
    remainder: Vec<String>,
    params: HashMap<String, String>,
    route: Route,
}

impl From<&Route> for RouteMatch {
    fn from(route: &Route) -> Self {
        Self {
            path: vec![],
            remainder: vec![],
            params: HashMap::new(),
            route: route.clone(),
        }
    }
}

impl RouteMatch {
    pub fn path(&self) -> String {
        self.path.join("/")
    }

    pub fn remainder(&self) -> Vec<String> {
        self.remainder.clone()
    }

    pub fn route(&self) -> &Route {
        &self.route
    }
}

impl PartialEq for RouteMatch {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

#[derive(Debug, Clone)]
pub struct Route {
    path: Vec<Segment>,
    resolver: fn() -> Dom,
}

impl Route {
    pub fn new(path: &str, resolver: fn() -> Dom) -> Self {
        Self {
            path: Parser::parse(path),
            resolver,
        }
    }

    pub fn resolve(&self) -> Dom {
        (self.resolver)()
    }

    pub fn matches(&self, sample: &Vec<String>) -> Option<RouteMatch> {
        let mut p = self.path.iter();
        let mut s = sample.iter();
        let mut mtch = RouteMatch::from(self);

        loop {
            match (p.next(), s.next()) {
                (Some(Segment::Static(seg)), Some(s)) if seg == s => {
                    mtch.path.push(s.to_string());
                }
                (Some(Segment::Param(p)), Some(s)) => {
                    mtch.params.insert(p.to_string(), s.to_string());
                    mtch.path.push(s.to_string());
                }
                (Some(Segment::Continue), Some(s)) => {
                    mtch.remainder.push(s.to_string());
                }
                (Some(Segment::Continue), None) => {
                    break;
                }
                (None, Some(s)) if !mtch.remainder.is_empty() => {
                    mtch.remainder.push(s.to_string());
                }
                (None, None) => {
                    break;
                }
                _ => {
                    return None;
                }
            }
        }

        Some(mtch)
    }
}

struct Parser<'p> {
    input: &'p str,
    index: usize,
}

impl<'p> Parser<'p> {
    pub(crate) fn parse(path: &'p str) -> Vec<Segment> {
        let mut result = vec![];

        let mut p = Self {
            input: path,
            index: 0,
        };

        loop {
            if p.peek() == '/' {
                p.consume_char();
            }

            if p.eol() {
                break;
            }

            match p.parse_segment() {
                Some(Segment::Continue) => {
                    result.push(Segment::Continue);
                    break;
                }
                Some(seg) => result.push(seg),
                None => (),
            }
        }

        result
    }

    fn parse_segment(&mut self) -> Option<Segment> {
        match self.peek() {
            '{' => self.parse_param(),
            '.' => self.parse_continue(),
            _ => self.parse_static(),
        }
    }

    fn parse_static(&mut self) -> Option<Segment> {
        match self.consume_while(|c| c != '/') {
            s if s.is_empty() => None,
            s => Some(Segment::Static(s)),
        }
    }

    fn parse_param(&mut self) -> Option<Segment> {
        self.consume_char();

        match self.consume_while(|c| c != '}' && c != '/') {
            s if s.is_empty() => None,
            s => {
                self.consume_char();
                Some(Segment::Param(s))
            }
        }
    }

    fn parse_continue(&mut self) -> Option<Segment> {
        match self.consume_while(|c| c == '.').as_str() {
            "..." => Some(Segment::Continue),
            _ => None,
        }
    }

    fn consume_while<F>(&mut self, cond: F) -> String
    where
        F: Fn(char) -> bool,
    {
        let mut result = String::new();

        while !self.eol() && cond(self.peek()) {
            result.push(self.consume_char());
        }

        result
    }

    fn consume_char(&mut self) -> char {
        self.index += 1;
        self.input.chars().nth(self.index - 1).unwrap_or_default()
    }

    fn eol(&self) -> bool {
        self.index >= self.input.len()
    }

    fn peek(&self) -> char {
        self.input.chars().nth(self.index).unwrap_or_default()
    }
}

pub fn split_path(p: &str) -> Vec<String> {
    p.split('/')
        .filter_map(|s| {
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .collect()
}
