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
    pub is_on_active_path: bool,
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
    pub root_path: String,
}

const TEMPLATE: &str = include_str!("page.html");

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
            is_on_active_path: false,
            section_type: None,
        }
    }

    fn ungrouped(items: Vec<NavItemData>) -> NavGroupData {
        NavGroupData { label: None, items }
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
            root_path: ".".to_owned(),
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
            root_path: "../..".to_owned(),
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
            root_path: ".".to_owned(),
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
            root_path: ".".to_owned(),
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
            root_path: ".".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("td-nav-link--active"));
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
            root_path: ".".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("td-toc-item--nested"));
        assert!(html.contains("#subsection"));
        assert!(html.contains("#section"));
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
            root_path: ".".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("nav-chevron"));
        assert!(html.contains("Billing"));
        assert!(html.contains("td-nav-children"));
    }

    #[test]
    fn render_nav_with_children_uses_details() {
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
            root_path: ".".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("<details>"));
        assert!(html.contains("<summary"));
        assert!(html.contains("nav-chevron"));
        assert!(html.contains("Billing"));
    }

    #[test]
    fn render_nav_active_path_sets_details_open() {
        let mut child = nav_item("Billing", "/domains/billing");
        child.is_active = true;
        child.is_on_active_path = true;
        let mut parent = nav_item("Domains", "/domains");
        parent.children = vec![child];
        parent.is_on_active_path = true;
        let page = PageData {
            title: "Billing".to_owned(),
            path: "domains/billing".to_owned(),
            html_content: String::new(),
            breadcrumbs: vec![],
            toc: vec![],
            scope: None,
            nav_groups: vec![ungrouped(vec![parent])],
            css_path: "../../assets/styles.css".to_owned(),
            root_path: "../..".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("<details open>"));
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
            root_path: "../..".to_owned(),
        };
        let html = render_page(&page);
        // Back link
        assert!(html.contains("Home"));
        assert!(html.contains("rotate-180")); // Left arrow
        // Section title
        assert!(html.contains("Billing"));
        assert!(html.contains("td-scope-title"));
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
            root_path: ".".to_owned(),
        };
        let html = render_page(&page);
        assert!(html.contains("Guide"));
        assert!(html.contains("Domains"));
        assert!(html.contains("td-nav-group-label"));
        assert!(html.contains("Billing"));
    }
}
