.. py:currentmodule:: starlark_tugger

.. _tugger_starlark_terminal:

===========================================
Functions for Interacting with the Terminal
===========================================

.. py:function:: can_prompt() -> bool

   Returns whether we are capable of prompting for user input.

   If this returns ``False``, functions like :py:func:`prompt_input` and
   :py:func:`prompt_password` will be unable to collect input from the user
   and will error unless a default value is provided.

.. py:function:: prompt_confirm(prompt: str, default: Optional[bool] = None) -> bool

   Prompt the user to confirm something.

   This will print the provided prompt and wait for user input to confirm it.

   If ``y`` or ``n`` is pressed, this evaluates to ``True`` or ``False``,
   respectively. If the escape key is pressed, an error is raised.

   If stdin is not interactive (e.g. it is connected to ``/dev/null``), this
   will return ``default`` if provided or raise an error otherwise.

.. py:function:: prompt_input(prompt: str, default: Optional[str] = None) -> str

   Prompt the user for input on the terminal.

   This will print a prompt with the given ``prompt`` text to stderr. If
   ``default`` is provided, the default value will printed and used if no input
   is provided.

   The string constituting the raw input (without a trailing newline) is
   returned.

   If stdin is not interactive (e.g. it is connected to ``/dev/null``), this
   will return ``default`` if provided or raise an error otherwise.

.. py:function:: prompt_password(prompt: str, confirm: bool = False, default: Optional[str] = None) -> str

   Prompt the user for a password input on the terminal.

   This will print a prompt with the given ``prompt`` text to stderr.

   When the user inputs their password, its content will not be printed back
   to the terminal.

   If ``confirm`` is ``True``, the user will be prompted to confirm the hidden
   value they just entered and subsequent prompts will be attempted until values
   agree.

   If stdin is not interactive (e.g. it is connected to ``/dev/null``), this
   will return ``default`` if provided or raise an error if not.

   The password value is stored in plain text in memory and treated like any
   other string value.
