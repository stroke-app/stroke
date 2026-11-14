### Improvements
- Model picker: Claude Code shows Claude's own mark, Ollama its own; moving the pointer across the provider icons no longer swaps the list under you; the search box is a proper combobox for screen readers; "Full access" is named as such and shown in amber
- AI models and Add model: one header and footer like the other dialogs; the provider step is a list with each provider's logo and one line on what it is, and picking one moves straight on
- The composer has a clear edge and a focus ring, the model chip and send button line up at one height, and the "↵ to send" hint is gone

### Bug Fixes
- A request sent without an API key says "No API key was sent" and offers AI settings, instead of "The provider rejected the API key" with OpenRouter's "No cookie auth credentials found"
- The AI error shows as a card in the composer's column instead of a red bar across the window
