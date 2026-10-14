An info string with a backtick needs a tilde fence:

~~~ a`b
x
~~~

Tilde runs in the content grow the fence:

~~~~~ a`b
~~~
~~~~
~~~~~

An info string that starts with a tilde keeps a space after the fence:

~~~ ~a`b
x
~~~

A tab-led backtick run in a list item cannot close the fence:

*   ```rust
    	```
    ```
