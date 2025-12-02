# Requirements

This is POC to test publishing markdown documents to Confluence.

We want to write ADRs using markdown in our `arch` repository and publish them
to Confluence for review. Confluence has rich commenting system and widely
adopted in our company.

This project is a test that it will work. How we will test:
- create simple md with 3 paragraphs of text
- create page with this md
- leave some comments on page
- change md (add one paragraph, change existing one)
- upload changes to page
- check that comments stay or some resolved

Use `~/Projects/adrflow` for reference how to connect to our Confluence.

## Current task

Next I need to publish markdown document from mkdocs site. It contains diagrams.
They must be rendered to PNG images and uploaded to Confluence with document.

mkdocs site: ~/Projects/invoices-migration/arch
document to upload: docs/domains/billing/adrs/adr-151/index.md
