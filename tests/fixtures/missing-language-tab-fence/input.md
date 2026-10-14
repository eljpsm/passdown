A tab's width depends on its column. These lines sat past the closing fence
in the source, and still must after the list marker narrows:

*   ```
    a
    	```
    b
    ```

1.  ~~~
    a
    	~~~
    b
    ~~~

In a quote in an ordered list:

1.  > ```
    > a
    >   	```
    > b
    > ```

In an ordered list in a quote:

> 1.    ```
>       a
>       	```
>       b
>       ```

A line with text after its backticks never closes a fence, so the fence does
not grow:

```
```python Python
x
```
