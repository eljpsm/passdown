A task item whose first block sits after a blank line keeps the blank, so the
block cannot join the checkbox line:

- [ ]

      indented code

- [x]

  <custom-x>
  y

- [ ]

  2. ordered from two

- [ ]

  -

- [ ]

  > quote

Looseness that comes only from the blank after the checkbox:

- [ ]

  ***

- b

Nested in a tight item:

- a
  - [ ]

        nested code
