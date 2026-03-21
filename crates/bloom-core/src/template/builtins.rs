//! Eight built-in templates that are always available regardless of the
//! user's `templates/` directory.

use super::template::{Template, TemplateEngine};

const BLANK: &str = "\
<!-- template: Blank page | Empty page with frontmatter -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: []
---

$0
";

const MEETING: &str = "\
<!-- template: Meeting notes | Notes for meetings with attendees, agenda, and action items -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: [meeting]
---

## Attendees
${1:Names}

## Agenda
${2:Topics}

## Notes
$0

## Action Items
- [ ] ${3:First action item}
";

const JOURNAL: &str = "\
<!-- template: Daily journal | Daily journal entry with prompts -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: []
---

## What happened today
${1:Events and observations}

## What I learned
${2:Insights}

## Tasks
- [ ] ${3:Task}

$0
";

const BOOK: &str = "\
<!-- template: Book review | Review a book with rating and takeaways -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: [book]
---

## Details
- **Author:** ${1:Author name}
- **Rating:** ${2:⭐⭐⭐⭐⭐}
- **Finished:** @at(${DATE})

## Key Takeaways
${3:Main ideas}

## Notes
$0

## Favourite Quotes
- ${4:Quote}
";

const PROJECT: &str = "\
<!-- template: Project page | Track a project with goals, tasks, and links -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: [project]
---

## Goals
${1:What this project aims to achieve}

## Tasks
- [ ] ${2:First task} @due(${DATE})
- [ ] ${3:Second task}

## Links
- ${4:Related pages}

## Notes
$0
";

const WEEKLY: &str = "\
<!-- template: Weekly review | Weekly review with task summary and reflection -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: [review, weekly]
---

## This Week's Wins
${1:What went well}

## Challenges
${2:What was difficult}

## Open Tasks
- [ ] ${3:Carry-over task}

## Next Week's Focus
${4:Priorities}

## Reflection
$0
";

const READING: &str = "\
<!-- template: Reading list | Track books and articles you're reading -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: [reading]
---

## Currently Reading
- ${1:Book/article title} @start(${DATE}) #reading

## Want to Read
- ${2:Title}

## Finished
$0
";

const MOVIE: &str = "\
<!-- template: Movie / TV log | Log movies and shows with ratings -->
---
id: ${AUTO}
title: \"${TITLE}\"
created: ${DATE}
tags: [movies]
---

## Details
- **Genre:** ${1:Genre}
- **Rating:** ${2:⭐⭐⭐⭐⭐}
- **Watched:** @at(${DATE})

## Review
${3:What I thought}

## Memorable Scenes
$0
";

/// `(name, description, content)` for every built-in template.
pub const BUILTIN_TEMPLATE_DATA: &[(&str, &str, &str)] = &[
    ("Blank page", "Empty page with frontmatter", BLANK),
    (
        "Meeting notes",
        "Notes for meetings with attendees, agenda, and action items",
        MEETING,
    ),
    ("Daily journal", "Daily journal entry with prompts", JOURNAL),
    (
        "Book review",
        "Review a book with rating and takeaways",
        BOOK,
    ),
    (
        "Project page",
        "Track a project with goals, tasks, and links",
        PROJECT,
    ),
    (
        "Weekly review",
        "Weekly review with task summary and reflection",
        WEEKLY,
    ),
    (
        "Reading list",
        "Track books and articles you're reading",
        READING,
    ),
    (
        "Movie / TV log",
        "Log movies and shows with ratings",
        MOVIE,
    ),
];

/// Return fully parsed [`Template`] objects for all built-in templates.
pub fn builtin_templates() -> Vec<Template> {
    BUILTIN_TEMPLATE_DATA
        .iter()
        .map(|(name, desc, content)| Template {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            content: (*content).to_string(),
            placeholders: TemplateEngine::parse_placeholders(content),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_count() {
        assert_eq!(builtin_templates().len(), 8);
    }

    #[test]
    fn blank_has_no_numbered_placeholders() {
        let templates = builtin_templates();
        let blank = templates.iter().find(|t| t.name == "Blank page").unwrap();
        // Only $0 (final cursor), no numbered placeholders.
        assert_eq!(
            blank
                .placeholders
                .iter()
                .filter(|p| p.index != 0)
                .count(),
            0
        );
    }

    #[test]
    fn meeting_has_placeholders() {
        let templates = builtin_templates();
        let meeting = templates
            .iter()
            .find(|t| t.name == "Meeting notes")
            .unwrap();
        // Should have numbered placeholders 1, 2, 3 plus $0.
        assert!(meeting.placeholders.len() >= 3);
    }

    #[test]
    fn all_have_magic_variables() {
        for t in builtin_templates() {
            assert!(
                t.content.contains("${AUTO}"),
                "{} missing ${{AUTO}}",
                t.name
            );
            assert!(
                t.content.contains("${DATE}"),
                "{} missing ${{DATE}}",
                t.name
            );
            assert!(
                t.content.contains("${TITLE}"),
                "{} missing ${{TITLE}}",
                t.name
            );
        }
    }
}
