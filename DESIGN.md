# Design notes for the web UI

The page is for one job: deciding whether two or three contact entries are
the same person, and what the one clean entry should say. People have done
this by hand for a century with a card file, so the page borrows from it:
each contact is an index card, the merged result is a fresh card in the
middle, and the tier of a group sits on a divider tab like the letters in an
address book.

## Colour

Light only. The cards are white paper and need a ground to sit on.

| Token | Value | Role |
|---|---|---|
| `--ground` | `#e4e9ee` | Page. A cool grey-blue, like a steel card drawer, so white cards stand off it. |
| `--card` | `#ffffff` | Every contact card and panel. |
| `--ink` | `#1d2733` | Text. 15.1:1 on cards, 12.4:1 on the ground. |
| `--muted` | `#4f5b68` | Labels, file names, help text. 6.9:1 on cards, 5.7:1 on the ground. |
| `--rule` | `#c0322b` | The red heading rule on each card, and errors. 5.6:1 on cards, 4.6:1 on the ground. |
| `--pen` | `#1f56a8` | Blue ballpoint: the value picked for the merged card, the main button, focus rings. 7.1:1 on cards; white on it is 7.1:1 too. |
| `--line` | `#c8d2dc` | Faint ruled lines on cards, borders, dividers. Never carries text. |

Red and blue are what index cards are printed with, so the two accents come
with the subject instead of being picked from a palette.

## Type

Public Sans (400, 600, 700) for the interface. It was made for US government
forms and reads plainly at small sizes, which suits labels like "mobile" and
"work" next to every value.

Courier Prime (400, 700) for the contact data itself: names, numbers,
addresses, file names and line numbers. Typed cards are the reference, and a
fixed-width face makes two phone numbers that differ by one digit easy to
compare down a column.

## Shape and space

Cards have a 2px radius, close to the square corners of real card stock.
Buttons get 4px. Spacing steps are 4, 8, 12, 16, 24 and 32px. No shadows
except a 1px offset under cards, the way a card sits on a desk.

Each card has a red rule under the name and pale ruled lines behind its
fields. The merged card is the only one with no source line in its corner:
it doesn't exist in any file yet.

## Motion

Motion only shows a change of state:

- moving to the next or previous group slides the cards in from that side
  (160 ms), so you can tell which way you went;
- a contact split out of a group dims and its card shrinks a little (150 ms).

With `prefers-reduced-motion` both are instant.

## Layout

Desktop: the group's cards side by side with the merged card in the middle,
reasons below, decision buttons in a bar that stays in view. With three or
more cards the extra ones go on the right. At phone width the merged card
comes first, then each original, then the reasons, and the decision bar
sticks to the bottom of the screen.

The overview leads with what was read and what was found, in that order,
because the first question after dropping files is "did it read all of
them?". Problems that aren't duplicates come after.

## Voice

The page talks to one person about their own contacts. Plain words, sentence
case, no exclamation marks. Words to avoid: smart, magic, seamless,
effortless, powerful. Errors say what happened and what to do next, for
example "work.csv line 14 has 3 columns but the header has 88. The rest of the
file was read."
