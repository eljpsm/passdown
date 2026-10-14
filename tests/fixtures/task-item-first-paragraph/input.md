A task item whose first paragraph sits after a blank line keeps the blank, so
the paragraph does not join the checkbox line:

- [ ]

  para

Ordered, checked, and followed by another item:

1. [x]

   para
2. b

Nested, with a paragraph that wraps:

- a
  - [ ]

    a paragraph that runs well past the eighty column limit of the style, so it wraps

A paragraph that starts on the checkbox line still joins it:

- [ ]
  para
- b

A checkbox below a blank marker line still joins its paragraph:

-
  [ ] para

In a tight list, a paragraph after a footnote definition[^h] joins the checkbox
line:

- [ ]
  [^h]: # Heading
  para

A checkbox after a definition[^n] stays a checkbox:

- [^n]: # Note
  [x] done
