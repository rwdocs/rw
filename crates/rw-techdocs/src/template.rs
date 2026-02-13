//! HTML page template for static site generation.
//!
//! Mirrors the Svelte frontend's DOM structure and Tailwind CSS classes
//! to produce pixel-perfect static HTML pages.

use std::fmt::Write;

/// Data for rendering a navigation item in the static template.
pub struct NavItemData {
    pub title: String,
    pub path: String,
    pub children: Vec<NavItemData>,
    pub is_active: bool,
    pub section_type: Option<String>,
}

/// Data for the scope header shown in scoped navigation.
pub struct ScopeHeaderData {
    /// Section title (e.g., "Billing").
    pub title: String,
    /// Back link label ("Home" or parent section title).
    pub back_link_title: String,
    /// Back link target path.
    pub back_link_path: String,
}

/// A group of navigation items with an optional type label.
pub struct NavGroupData {
    /// Group label (e.g., "Systems"). `None` for ungrouped items.
    pub label: Option<String>,
    pub items: Vec<NavItemData>,
}

/// Data for a breadcrumb entry.
pub struct BreadcrumbData {
    pub title: String,
    pub path: String,
}

/// Data for a table of contents entry.
pub struct TocData {
    pub level: u8,
    pub title: String,
    pub id: String,
}

/// All data needed to render a static page.
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

/// Render a complete static HTML page.
pub fn render_page(page: &PageData) -> String {
    let mut html = String::with_capacity(8192);

    // DOCTYPE and head
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = write!(html, "<title>{}</title>\n", escape(&page.title));
    let _ = write!(
        html,
        "<link rel=\"stylesheet\" href=\"{}\">\n",
        &page.css_path
    );
    // Breadcrumb separator style (matches Breadcrumbs.svelte scoped CSS)
    html.push_str("<style>\n");
    html.push_str(".breadcrumb-item::after {\n");
    html.push_str("  content: \"/\";\n");
    html.push_str("  margin-left: 0.625rem;\n");
    html.push_str("  margin-right: 0.5rem;\n");
    html.push_str("  color: rgb(156 163 175);\n");
    html.push_str("}\n");
    html.push_str(".breadcrumb-item:last-child::after {\n");
    html.push_str("  content: none;\n");
    html.push_str("}\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body class=\"bg-white text-gray-900 antialiased\">\n");

    // Main layout container (matches Layout.svelte)
    html.push_str("<div class=\"min-h-screen flex flex-col md:flex-row\">\n");

    // Navigation sidebar
    render_sidebar(&mut html, page.scope.as_ref(), &page.nav_groups);

    // Content + ToC container
    html.push_str("<div class=\"flex-1\">\n");
    html.push_str("<div class=\"max-w-6xl mx-auto px-4 md:px-8 pt-6 pb-12\">\n");

    // Breadcrumbs
    render_breadcrumbs(&mut html, &page.breadcrumbs);

    html.push_str("<div class=\"flex\">\n");

    // Main content (matches PageContent.svelte)
    html.push_str("<main class=\"flex-1 min-w-0\">\n");
    html.push_str("<article class=\"prose prose-slate max-w-none\">\n");
    html.push_str(&page.html_content);
    html.push_str("\n</article>\n</main>\n");

    // ToC sidebar
    render_toc(&mut html, &page.toc);

    html.push_str("</div>\n</div>\n</div>\n</div>\n");
    html.push_str("</body>\n</html>");
    html
}

/// Render the navigation sidebar (matches Layout.svelte aside).
fn render_sidebar(html: &mut String, scope: Option<&ScopeHeaderData>, groups: &[NavGroupData]) {
    html.push_str(
        "<aside class=\"w-[280px] flex-shrink-0 border-r border-gray-200 \
         hidden md:block h-screen sticky top-0 overflow-y-auto\">\n",
    );
    html.push_str("<div class=\"pt-6 px-4 pb-4\">\n");

    // Logo
    html.push_str("<a href=\"/\" class=\"block mb-5 pl-[6px]\">\n");
    html.push_str("<span class=\"text-xl font-semibold uppercase\">");
    html.push_str("<span class=\"text-gray-900\">R</span>");
    html.push_str("<span class=\"text-gray-400\">W</span>");
    html.push_str("</span>\n</a>\n");

    // Scope header (matches NavigationSidebar.svelte)
    if let Some(scope) = scope {
        render_scope_header(html, scope);
    }

    // Navigation tree with groups
    html.push_str("<nav>\n");
    render_nav_groups(html, groups);
    html.push_str("</nav>\n");

    html.push_str("</div>\n</aside>\n");
}

/// Render the scope header with back link and section title.
fn render_scope_header(html: &mut String, scope: &ScopeHeaderData) {
    html.push_str("<div class=\"mb-5\">\n");

    // Back link with left arrow
    let _ = write!(
        html,
        "<a href=\"{}\" class=\"text-sm text-gray-500 hover:text-blue-600 \
         flex items-center mb-2\">\n",
        escape(&scope.back_link_path),
    );
    html.push_str("<span class=\"w-[22px] flex items-center justify-center\">\n");
    html.push_str(
        "<svg class=\"w-3.5 h-3.5 rotate-180\" fill=\"currentColor\" \
         viewBox=\"0 0 20 20\">\n",
    );
    html.push_str(
        "<path fill-rule=\"evenodd\" d=\"M7.293 14.707a1 1 0 010-1.414L10.586 \
         10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 \
         01-1.414 0z\" clip-rule=\"evenodd\"/>\n",
    );
    html.push_str("</svg>\n</span>\n");
    let _ = write!(
        html,
        "<span class=\"px-1.5\">{}</span>\n",
        escape(&scope.back_link_title),
    );
    html.push_str("</a>\n");

    // Section title
    let _ = write!(
        html,
        "<h2 class=\"text-xl font-light text-gray-900 pl-[28px]\">{}</h2>\n",
        escape(&scope.title),
    );

    html.push_str("</div>\n");
}

/// Render navigation groups (matches NavTree.svelte + NavGroup.svelte).
fn render_nav_groups(html: &mut String, groups: &[NavGroupData]) {
    for group in groups {
        if let Some(label) = &group.label {
            // Labeled group (matches NavGroup.svelte with label)
            html.push_str("<div class=\"nav-group mt-5 first:mt-0\">\n");
            let _ = write!(
                html,
                "<div class=\"text-xs font-semibold text-gray-500 uppercase \
                 tracking-wider px-1.5 pb-1.5\">{}</div>\n",
                escape(label),
            );
            html.push_str("<ul>\n");
            render_nav_items(html, &group.items);
            html.push_str("</ul>\n");
            html.push_str("</div>\n");
        } else {
            // Ungrouped items
            html.push_str("<ul>\n");
            render_nav_items(html, &group.items);
            html.push_str("</ul>\n");
        }
    }
}

/// Render navigation items recursively (matches NavItem.svelte).
fn render_nav_items(html: &mut String, items: &[NavItemData]) {
    for item in items {
        html.push_str("<li>\n");
        html.push_str("<div class=\"flex items-center\">\n");

        if item.children.is_empty() {
            // Spacer matching button width
            html.push_str("<span class=\"w-[22px]\"></span>\n");
        } else {
            // Expanded chevron (static: always show children)
            html.push_str(
                "<span class=\"w-5 h-5 flex items-center justify-center \
                 text-gray-500 mr-0.5\">\n",
            );
            html.push_str(
                "<svg class=\"w-3.5 h-3.5 rotate-90\" fill=\"currentColor\" \
                 viewBox=\"0 0 20 20\">\n",
            );
            html.push_str(
                "<path fill-rule=\"evenodd\" d=\"M7.293 14.707a1 1 0 010-1.414L10.586 \
                 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 \
                 01-1.414 0z\" clip-rule=\"evenodd\"/>\n",
            );
            html.push_str("</svg>\n</span>\n");
        }

        // Item link
        let active_classes = if item.is_active {
            "text-blue-700 font-medium"
        } else {
            "text-gray-700 hover:text-gray-900"
        };
        let _ = write!(
            html,
            "<a href=\"{}\" class=\"flex-1 py-1.5 px-1.5 rounded text-sm {}\">{}</a>\n",
            escape(&item.path),
            active_classes,
            escape(&item.title),
        );

        html.push_str("</div>\n");

        // Nested children
        if !item.children.is_empty() {
            html.push_str("<ul class=\"ml-3\">\n");
            render_nav_items(html, &item.children);
            html.push_str("</ul>\n");
        }

        html.push_str("</li>\n");
    }
}

/// Render breadcrumbs (matches Breadcrumbs.svelte).
fn render_breadcrumbs(html: &mut String, breadcrumbs: &[BreadcrumbData]) {
    if breadcrumbs.is_empty() {
        return;
    }
    html.push_str("<nav class=\"mb-6\">\n");
    html.push_str("<ol class=\"flex items-center text-sm text-gray-600\">\n");
    for crumb in breadcrumbs {
        html.push_str("<li class=\"breadcrumb-item\">\n");
        let _ = write!(
            html,
            "<a href=\"{}\" class=\"hover:text-gray-700 hover:underline\">{}</a>\n",
            escape(&crumb.path),
            escape(&crumb.title),
        );
        html.push_str("</li>\n");
    }
    html.push_str("</ol>\n</nav>\n");
}

/// Render the table of contents sidebar (matches TocSidebar.svelte).
fn render_toc(html: &mut String, toc: &[TocData]) {
    if toc.is_empty() {
        return;
    }
    html.push_str("<aside class=\"w-[240px] flex-shrink-0 hidden lg:block\">\n");
    html.push_str("<div class=\"pl-8 sticky top-6\">\n");
    html.push_str(
        "<h3 class=\"text-xs font-semibold text-gray-600 uppercase \
         tracking-wider mb-3\">On this page</h3>\n",
    );
    html.push_str("<ul class=\"space-y-1.5\">\n");
    for entry in toc {
        let indent = if entry.level >= 3 {
            " class=\"ml-3\""
        } else {
            ""
        };
        let _ = write!(
            html,
            "<li{}><a href=\"#{}\" class=\"block text-sm leading-snug \
             text-gray-600 hover:text-gray-900\">{}</a></li>\n",
            indent,
            escape(&entry.id),
            escape(&entry.title),
        );
    }
    html.push_str("</ul>\n</div>\n</aside>\n");
}

/// Escape HTML special characters.
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
