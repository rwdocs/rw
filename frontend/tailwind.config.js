/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,svelte}"],
  theme: {
    extend: {
      fontFamily: {
        sans: [
          'Roboto',
          'system-ui',
          '-apple-system',
          'BlinkMacSystemFont',
          'Segoe UI',
          'sans-serif',
        ],
        mono: [
          'JetBrains Mono',
          'ui-monospace',
          'SFMono-Regular',
          'Menlo',
          'Monaco',
          'Consolas',
          'monospace',
        ],
      },
      typography: {
        DEFAULT: {
          css: {
            maxWidth: '70ch',
            // Headings
            'h1, h2, h3, h4': {
              letterSpacing: '-0.025em',
              fontWeight: '600',
            },
            h2: {
              marginTop: '2em',
              marginBottom: '1em',
            },
            h3: {
              marginTop: '1.6em',
              marginBottom: '0.6em',
            },
            // Links
            a: {
              color: 'rgb(37 99 235)', // blue-600
              textDecoration: 'none',
              transition: 'color 0.15s',
              '&:hover': {
                color: 'rgb(29 78 216)', // blue-700
                textDecoration: 'underline',
                textUnderlineOffset: '3px',
                textDecorationThickness: '1px',
              },
            },
            // Inline code
            code: {
              fontFamily: 'JetBrains Mono, ui-monospace, monospace',
              fontSize: '0.875em',
              fontWeight: '400',
              backgroundColor: 'rgb(248 250 252)', // slate-50
              padding: '0.2em 0.4em',
              borderRadius: '0.25rem',
              border: '1px solid rgb(226 232 240)', // slate-200
            },
            'code::before': { content: '""' },
            'code::after': { content: '""' },
            // Code blocks
            pre: {
              fontFamily: 'JetBrains Mono, ui-monospace, monospace',
              fontSize: '0.875rem',
              lineHeight: '1.7',
              backgroundColor: 'rgb(248 250 252)', // slate-50
              color: 'rgb(30 41 59)', // slate-800
              border: '1px solid rgb(226 232 240)', // slate-200
              borderRadius: '0.5rem',
              padding: '1rem 1.25rem',
            },
            'pre code': {
              backgroundColor: 'transparent',
              color: 'inherit',
              padding: '0',
              border: 'none',
              borderRadius: '0',
              fontSize: 'inherit',
            },
            // Blockquotes
            blockquote: {
              fontStyle: 'normal',
              fontWeight: '400',
              borderLeftColor: 'rgb(203 213 225)', // slate-300
              borderLeftWidth: '3px',
              paddingLeft: '1em',
              color: 'rgb(71 85 105)', // slate-600
            },
            'blockquote p:first-of-type::before': { content: '""' },
            'blockquote p:last-of-type::after': { content: '""' },
            // Tables
            table: {
              fontSize: '0.875rem',
            },
            thead: {
              borderBottomColor: 'rgb(203 213 225)', // slate-300
            },
            'thead th': {
              fontWeight: '600',
              paddingBottom: '0.75rem',
            },
            'tbody tr': {
              borderBottomColor: 'rgb(226 232 240)', // slate-200
            },
            'tbody td': {
              paddingTop: '0.75rem',
              paddingBottom: '0.75rem',
            },
            // Horizontal rules
            hr: {
              borderColor: 'rgb(226 232 240)', // slate-200
              marginTop: '2.5em',
              marginBottom: '2.5em',
            },
            // Lists
            'ul > li::marker': {
              color: 'rgb(148 163 184)', // slate-400
            },
            'ol > li::marker': {
              color: 'rgb(100 116 139)', // slate-500
            },
          },
        },
      },
    },
  },
  plugins: [require("@tailwindcss/typography")],
};
