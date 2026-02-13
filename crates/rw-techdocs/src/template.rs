//! HTML page template for static site generation.
//!
//! Mirrors the Svelte frontend's DOM structure and Tailwind CSS classes
//! to produce pixel-perfect static HTML pages.
//!
//! Uses a minijinja template for rendering.

use minijinja::Environment;
use serde::Serialize;

/// Data for rendering a navigation item in the static template.
#[derive(Serialize)]
pub struct NavItemData {
    pub title: String,
    pub path: String,
    pub children: Vec<NavItemData>,
    pub is_active: bool,
    pub section_type: Option<String>,
}

/// Data for the scope header shown in scoped navigation.
#[derive(Serialize)]
pub struct ScopeHeaderData {
    /// Section title (e.g., "Billing").
    pub title: String,
    /// Back link label ("Home" or parent section title).
    pub back_link_title: String,
    /// Back link target path.
    pub back_link_path: String,
}

/// A group of navigation items with an optional type label.
#[derive(Serialize)]
pub struct NavGroupData {
    /// Group label (e.g., "Systems"). `None` for ungrouped items.
    pub label: Option<String>,
    pub items: Vec<NavItemData>,
}

/// Data for a breadcrumb entry.
#[derive(Serialize)]
pub struct BreadcrumbData {
    pub title: String,
    pub path: String,
}

/// Data for a table of contents entry.
#[derive(Serialize)]
pub struct TocData {
    pub level: u8,
    pub title: String,
    pub id: String,
}

/// All data needed to render a static page.
#[derive(Serialize)]
pub struct PageData {
    pub title: String,
    pub path: String,
    pub html_content: String,
    pub breadcrumbs: Vec<BreadcrumbData>,
    pub toc: Vec<TocData>,
    pub scope: Option<ScopeHeaderData>,
    pub nav_groups: Vec<NavGroupData>,
    pub css_path: String,
}

const TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ title }}</title>
<link rel="stylesheet" href="{{ css_path }}">
<style>
.breadcrumb-item::after {
  content: "/";
  margin-left: 0.625rem;
  margin-right: 0.5rem;
  color: rgb(156 163 175);
}
.breadcrumb-item:last-child::after {
  content: none;
}
</style>
</head>
<body class="bg-white text-gray-900 antialiased">
<div class="min-h-screen flex flex-col md:flex-row">
<aside class="w-[280px] flex-shrink-0 border-r border-gray-200 hidden md:block h-screen sticky top-0 overflow-y-auto">
<div class="pt-6 px-4 pb-4">
<a href="/" class="block mb-5 pl-[6px]">
<span class="text-xl font-semibold uppercase"><span class="text-gray-900">R</span><span class="text-gray-400">W</span></span>
</a>
{%- if scope %}
<div class="mb-5">
<a href="{{ scope.back_link_path }}" class="text-sm text-gray-500 hover:text-blue-600 flex items-center mb-2">
<span class="w-[22px] flex items-center justify-center">
<svg class="w-3.5 h-3.5 rotate-180" fill="currentColor" viewBox="0 0 20 20">
<path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
</svg>
</span>
<span class="px-1.5">{{ scope.back_link_title }}</span>
</a>
<h2 class="text-xl font-light text-gray-900 pl-[28px]">{{ scope.title }}</h2>
</div>
{%- endif %}
<nav>
{%- for group in nav_groups %}
{%- if group.label %}
<div class="nav-group mt-5 first:mt-0">
<div class="text-xs font-semibold text-gray-500 uppercase tracking-wider px-1.5 pb-1.5">{{ group.label }}</div>
<ul>
{%- for item in group.items recursive %}
<li>
<div class="flex items-center">
{%- if item.children %}
<span class="w-5 h-5 flex items-center justify-center text-gray-500 mr-0.5">
<svg class="w-3.5 h-3.5 rotate-90" fill="currentColor" viewBox="0 0 20 20">
<path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
</svg>
</span>
{%- else %}
<span class="w-[22px]"></span>
{%- endif %}
<a href="{{ item.path }}" class="flex-1 py-1.5 px-1.5 rounded text-sm {% if item.is_active %}text-blue-700 font-medium{% else %}text-gray-700 hover:text-gray-900{% endif %}">{{ item.title }}</a>
</div>
{%- if item.children %}
<ul class="ml-3">
{{ loop(item.children) }}
</ul>
{%- endif %}
</li>
{%- endfor %}
</ul>
</div>
{%- else %}
<ul>
{%- for item in group.items recursive %}
<li>
<div class="flex items-center">
{%- if item.children %}
<span class="w-5 h-5 flex items-center justify-center text-gray-500 mr-0.5">
<svg class="w-3.5 h-3.5 rotate-90" fill="currentColor" viewBox="0 0 20 20">
<path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
</svg>
</span>
{%- else %}
<span class="w-[22px]"></span>
{%- endif %}
<a href="{{ item.path }}" class="flex-1 py-1.5 px-1.5 rounded text-sm {% if item.is_active %}text-blue-700 font-medium{% else %}text-gray-700 hover:text-gray-900{% endif %}">{{ item.title }}</a>
</div>
{%- if item.children %}
<ul class="ml-3">
{{ loop(item.children) }}
</ul>
{%- endif %}
</li>
{%- endfor %}
</ul>
{%- endif %}
{%- endfor %}
</nav>
</div>
</aside>
<div class="flex-1">
<div class="max-w-6xl mx-auto px-4 md:px-8 pt-6 pb-12">
{%- if breadcrumbs %}
<nav class="mb-6">
<ol class="flex items-center text-sm text-gray-600">
{%- for crumb in breadcrumbs %}
<li class="breadcrumb-item">
<a href="{{ crumb.path }}" class="hover:text-gray-700 hover:underline">{{ crumb.title }}</a>
</li>
{%- endfor %}
</ol>
</nav>
{%- endif %}
<div class="flex">
<main class="flex-1 min-w-0">
<article class="prose prose-slate max-w-none">
{{ html_content|safe }}
</article>
</main>
{%- if toc %}
<aside class="w-[240px] flex-shrink-0 hidden lg:block">
<div class="pl-8 sticky top-6">
<h3 class="text-xs font-semibold text-gray-600 uppercase tracking-wider mb-3">On this page</h3>
<ul class="space-y-1.5">
{%- for entry in toc %}
<li{% if entry.level >= 3 %} class="ml-3"{% endif %}><a href="#{{ entry.id }}" class="block text-sm leading-snug text-gray-600 hover:text-gray-900">{{ entry.title }}</a></li>
{%- endfor %}
</ul>
</div>
</aside>
{%- endif %}
</div>
</div>
</div>
</div>
</body>
</html>"##;

/// Render a complete static HTML page.
pub fn render_page(page: &PageData) -> String {
    let mut env = Environment::new();
    env.add_template("page", TEMPLATE)
        .expect("invalid template");
    let tmpl = env.get_template("page").expect("template not found");
    tmpl.render(minijinja::value::Value::from_serialize(page))
        .expect("template rendering failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav_item(title: &str, path: &str) -> NavItemData {
        NavItemData {
            title: title.to_owned(),
            path: path.to_owned(),
            children: vec![],
            is_active: false,
            section_type: None,
        }
    }

    fn ungrouped(items: Vec<NavItemData>) -> NavGroupData {
        NavGroupData { label: None, items }
    }

    /// Escape HTML special characters (test-only, replaced by minijinja auto-escaping).
    fn escape(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '&' => result.push_str("&amp;"),
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '"' => result.push_str("&quot;"),
                '\'' => result.push_str("&#x27;"),
                _ => result.push(c),
            }
        }
        result
    }

    #[test]
    fn render_page_contains_content() {
        let page = PageData {
            title: "My Page".to_owned(),
            path: "guide".to_owned(),
            html_content: "<p>Hello world</p>".to_owned(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: None,
            nav_groups: vec![],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("<p>Hello world</p>"));
        assert!(html.contains("<title>My Page</title>"));
        assert!(html.contains("assets/styles.css"));
    }

    #[test]
    fn render_page_contains_breadcrumbs() {
        let page = PageData {
            title: "API".to_owned(),
            path: "domains/billing/api".to_owned(),
            html_content: "<p>API docs</p>".to_owned(),
            breadcrumbs: vec![
                BreadcrumbData {
                    title: "Domains".to_owned(),
                    path: "/domains".to_owned(),
                },
                BreadcrumbData {
                    title: "Billing".to_owned(),
                    path: "/domains/billing".to_owned(),
                },
            ],
            toc: vec![],
            scope: None,
            nav_groups: vec![],
            css_path: "../../assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("Domains"));
        assert!(html.contains("Billing"));
        assert!(html.contains("/domains"));
    }

    #[test]
    fn render_page_contains_toc() {
        let page = PageData {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            html_content: "<h2 id=\"intro\">Intro</h2>".to_owned(),
            breadcrumbs: vec![],
            toc: vec![TocData {
                level: 2,
                title: "Intro".to_owned(),
                id: "intro".to_owned(),
            }],
            scope: None,
            nav_groups: vec![],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("On this page"));
        assert!(html.contains("#intro"));
        assert!(html.contains("Intro"));
    }

    #[test]
    fn render_page_contains_navigation() {
        let page = PageData {
            title: "Home".to_owned(),
            path: String::new(),
            html_content: "<p>Home</p>".to_owned(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: None,
            nav_groups: vec![ungrouped(vec![nav_item("Guide", "/guide")])],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("Guide"));
        assert!(html.contains("/guide"));
    }

    #[test]
    fn render_page_marks_active_nav_item() {
        let mut item = nav_item("Guide", "/guide");
        item.is_active = true;
        let page = PageData {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            html_content: "<p>Guide</p>".to_owned(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: None,
            nav_groups: vec![ungrouped(vec![item])],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("text-blue-700 font-medium"));
    }

    #[test]
    fn render_page_indents_toc_level_3() {
        let page = PageData {
            title: "Guide".to_owned(),
            path: "guide".to_owned(),
            html_content: String::new(),
            breadcrumbs: vec![],
            toc: vec![
                TocData {
                    level: 2,
                    title: "Section".to_owned(),
                    id: "section".to_owned(),
                },
                TocData {
                    level: 3,
                    title: "Subsection".to_owned(),
                    id: "subsection".to_owned(),
                },
            ],
            scope: None,
            nav_groups: vec![],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("<li class=\"ml-3\"><a href=\"#subsection\""));
        assert!(html.contains("<li><a href=\"#section\""));
    }

    #[test]
    fn escape_special_characters() {
        assert_eq!(escape("<script>"), "&lt;script&gt;");
        assert_eq!(escape("a&b"), "a&amp;b");
        assert_eq!(escape("\"hello\""), "&quot;hello&quot;");
    }

    #[test]
    fn render_page_nav_with_children_shows_chevron() {
        let mut parent = nav_item("Domains", "/domains");
        parent.children = vec![nav_item("Billing", "/domains/billing")];
        let page = PageData {
            title: "Home".to_owned(),
            path: String::new(),
            html_content: String::new(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: None,
            nav_groups: vec![ungrouped(vec![parent])],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("rotate-90"));
        assert!(html.contains("Billing"));
        assert!(html.contains("<ul class=\"ml-3\">"));
    }

    #[test]
    fn render_scope_header_shows_back_link_and_title() {
        let page = PageData {
            title: "API".to_owned(),
            path: "domains/billing/api".to_owned(),
            html_content: String::new(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: Some(ScopeHeaderData {
                title: "Billing".to_owned(),
                back_link_title: "Home".to_owned(),
                back_link_path: "/".to_owned(),
            }),
            nav_groups: vec![ungrouped(vec![nav_item("API", "/domains/billing/api")])],
            css_path: "../../assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        // Back link
        assert!(html.contains("Home"));
        assert!(html.contains("rotate-180")); // Left arrow
        // Section title
        assert!(html.contains("Billing"));
        assert!(html.contains("text-xl font-light"));
    }

    #[test]
    fn render_nav_groups_with_labels() {
        let mut billing = nav_item("Billing", "/domains/billing");
        billing.section_type = Some("domain".to_owned());
        let page = PageData {
            title: "Home".to_owned(),
            path: String::new(),
            html_content: String::new(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: None,
            nav_groups: vec![
                ungrouped(vec![nav_item("Guide", "/guide")]),
                NavGroupData {
                    label: Some("Domains".to_owned()),
                    items: vec![billing],
                },
            ],
            css_path: "assets/styles.css".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("Guide"));
        assert!(html.contains("Domains"));
        assert!(html.contains("uppercase tracking-wider"));
        assert!(html.contains("Billing"));
    }
}
