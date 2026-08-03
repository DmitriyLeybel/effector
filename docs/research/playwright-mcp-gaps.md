# Playwright MCP browser-management gaps

Status: research snapshot
Last reviewed: 2026-07-30
Reference: [Playwright MCP tab tools](https://playwright.dev/mcp/tools/tabs)

This table identifies browser-chrome and persistent-session operations that
complement Playwright's page-oriented automation. Revalidate it when the
Playwright MCP tool contract changes.

| Feature | Playwright MCP status | Could it be worked around? | Opportunity for our extension |
| --- | --- | --- | --- |
| Enumerate browser windows | Not supported by browser_tabs | Partial Chromium-only CDP workaround | Return every window with ID, type, focus and bounds |
| Associate tabs with windows | Not exposed | Possibly inferred through CDP, but not part of the MCP contract | Return an exact window → tabs hierarchy |
| Create, close and focus windows | No documented MCP tools | Some window bounds/focus operations may be possible through browser-level CDP | Native chrome.windows tools |
| List tab groups | Not supported | No normal Playwright Page API equivalent | Return group ID, title, color, collapsed state and window |
| Group and ungroup tabs | Not supported | Not reliably available through generic Playwright or normal CDP | Native chrome.tabs.group() and ungroup() |
| Rename, recolor or collapse groups | Not supported | No practical generic workaround | Native chrome.tabGroups.update() |
| Move groups between windows | Not supported | No practical generic workaround | Native chrome.tabGroups.move() |
| Show group membership | Not exposed | Cannot reconstruct reliably from title/URL/index | Include groupId in every tab record |
| Detect discarded tabs | Not exposed | No supported Playwright property; attaching may cause the tab to load | Return discarded: true without touching the tab |
| Detect frozen tabs | Not exposed | No supported Page property | Return frozen: true without activating it |
| Guarantee “never wake this tab” | Not documented | Playwright is oriented toward attaching to and automating pages | Make non-waking behavior an explicit API guarantee |
| Metadata access without a live renderer | Poor fit | A discarded page may not be usable as a Playwright Page | Read metadata directly from chrome.tabs |
| Report last-accessed time | Not exposed | No Page API equivalent | Return Chrome’s lastAccessed value |
| Report pinned state | Not exposed | Cannot reliably obtain from the page | Return and modify pinned |
| Pin or unpin tabs | Not supported | UI automation would be brittle | Native tab update |
| Report audible/muted state | Not exposed by browser_tabs | Audio can sometimes be manipulated through CDP, but it is not tab-strip state | Return audible, mutedInfo and mute ownership |
| Mute or unmute a Chrome tab | No documented tab tool | Possible partial CDP audio emulation, not equivalent to Chrome’s mute control | Native Chrome mute operation |
| Report favicon | Not exposed | Could scrape the document’s icon, but that is not necessarily Chrome’s current favicon | Return favIconUrl directly |
| Report loading/pending-navigation state | Not exposed in the tab list | Playwright can observe an attached page’s loading, but not as passive inventory | Return status and pendingUrl |
| Report incognito state | Not exposed | Context separation may hint at it, but not reliably | Return incognito, with explicit privacy policy |
| Report opener tab | Not exposed in the MCP tab listing | Sometimes inferable from page relationships | Return openerTabId where Chrome provides it |
| Use stable tab identifiers | Uses positional indexes | Pages can be retained in code, but the standard tool addresses tabs by changing array index | Expose tab IDs plus browser-session identity |
| Preserve exact tab-strip ordering per window | Only a flat tab index is documented | Page ordering is not a full representation of multiple Chrome windows | Return physical index within each window |
| Move a tab within the tab strip | Not supported | No ordinary Playwright equivalent | Native chrome.tabs.move() |
| Move a tab between windows | Not supported | Difficult and browser-specific through CDP | Native move with destination window ID |
| Duplicate a Chrome tab | No tab action | Opening the same URL is possible, but does not duplicate history/state like Chrome | Native chrome.tabs.duplicate() |
| Read recently closed tabs/windows | Not supported | Playwright only sees its current connected pages | Optional chrome.sessions integration |
| Restore recently closed tabs/windows | Not supported | Navigating to the old URL is not equivalent | Native session restoration |
| Subscribe to tab-strip changes | No MCP-level browser inventory event stream | Playwright emits Page events for attached contexts, but not Chrome group/window metadata | Emit tab/window/group creation, move and update events |
| See saved-but-closed tab groups | Not supported | Chrome itself lacks a complete normal extension API for this | Probably still unavailable to us |
| Inspect other Chrome profiles | Not supported | A separate connection is needed per profile | Pair one extension instance per profile and identify each instance |
| Inspect browser-internal pages | Generally restricted | CDP may provide limited target access, but normal page automation remains restricted | Mostly still unavailable; metadata only |
| URL-only fallback for an unloaded tab | Not a first-class workflow | The URL may appear in a tab list, but there is no explicit “do not attach; hand off URL” | Return a structured fetch recommendation without loading |
