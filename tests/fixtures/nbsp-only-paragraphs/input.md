A paragraph left empty by punctuation normalization keeps its characters as
references:

&nbsp;

   
​

&#32;

Between hard breaks:

&nbsp;  
&nbsp;

In list items, a quote and a task item:

-  
- b

1. &nbsp;
2. b

> &nbsp;

- [ ] &nbsp;
- [x] b

Prose still converts them: here.

Multi-line:

&nbsp;
&#32;

Inside inline markup: *&nbsp;*, **&nbsp;**, ~~&nbsp;~~, ***&nbsp;***,
[&nbsp;](https://example.com) and ![&nbsp;](a.png).

A line after a hard break:

foo  
&nbsp;

a  
&nbsp;  
b

# &nbsp;

| a      | b   |
| ------ | --- |
| &nbsp; | x   |
