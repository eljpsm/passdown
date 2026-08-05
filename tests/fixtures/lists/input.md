Adjacent lists that differ only by marker must not merge:

* star bullet
+ plus bullet
- dash bullet

Ordered markers are normalized and renumbered from the start value:

1) paren one
2) paren two

5. starts at five
6. continues

A tight list with nesting:

- tight item one
- tight item two
    - nested tight
    - another nested
- tight item three

A loose list:

- loose item one

- loose item two with a second paragraph

  second paragraph of the loose item

Task items:

- [ ] open task
- [X] done task

Mixed nesting:

1. ordered with nested unordered
    - inner dash
    - inner dash two
2. second

Wrapping inside an item:

- an item whose text is long enough that it must wrap across multiple lines to stay under the eighty column limit for continuation lines

An empty item stays a bare marker:

- first
-
- third

An empty task item and one whose first block is code:

- [ ]
- [ ]
  ```rust
  checked = false;
  ```

Sibling same-type lists inside one tight item:

- outer item
  * inner star
  - inner dash

Sibling same-type lists inside one loose item:

- outer loose item

  * inner star

  - inner dash
