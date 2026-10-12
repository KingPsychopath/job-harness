// Data is plain JSON. Never eval a candidate's text as Typst code.
#let data = json(sys.inputs.at("content", default: "resume.content.json"))
#let theme = data.at("theme", default: (:))
#let accent = rgb(theme.at("accent", default: "#287D72"))
#let ink = rgb(theme.at("ink", default: "#243444"))
#let point = theme.at("font_size", default: 9.2)
#set page(paper: "a4", margin: theme.at("margin_mm", default: 17) * 1mm)
#set text(font: theme.at("font", default: "Libertinus Serif"), size: point * 1pt, fill: ink)
#set par(justify: false, leading: 0.42em, spacing: 0.65em)
#set list(indent: 9pt, body-indent: 4pt, spacing: 3pt)
#show link: set text(fill: accent)
#let section(title, body) = {
  v(theme.at("section_gap_pt", default: 10) * 1pt)
  block(width: 100%)[
    #text(size: 9pt, weight: "bold", fill: accent, tracking: 0.8pt)[#upper(title)]
    #v(3pt)
    #line(length: 100%, stroke: 0.45pt + accent.lighten(65%))
    #v(4pt)
    #body
  ]
}
#let entry(item) = block(breakable: false)[
  #grid(columns: (1fr, auto), column-gutter: 10pt,
    text(weight: "bold")[#item.title],
    text(size: 8.6pt, fill: ink.lighten(30%))[#item.period],
  )
  #if item.at("url", default: none) != none {
    link(item.url)[#text(style: "italic")[#item.organisation]]
  } else {
    text(style: "italic")[#item.organisation]
  }
  #if item.bullets.len() > 0 {
    v(3pt)
    list(..item.bullets.map(bullet => [#bullet.text]))
  }
  #v(5pt)
]
#text(size: 27pt, weight: "bold")[#data.person.name]
#v(1pt)
#text(size: 11.5pt, fill: accent)[#data.person.headline]
#v(6pt)
#text(size: 8.6pt)[
  #data.person.location #h(5pt) · #h(5pt)
  #link("mailto:" + data.person.email)[#data.person.email]
  #for item in data.person.links [#h(5pt) · #h(5pt) #link(item.url)[#item.label]]
]
#v(8pt)
#data.summary.text
#section("Experience", [#for item in data.experience { entry(item) }])
#if data.projects.len() > 0 { section("Selected work", [#for item in data.projects { entry(item) }]) }
#if data.skills.len() > 0 { section("Skills", [#for item in data.skills [#item.text #parbreak()]]) }
#if data.education.len() > 0 { section("Education", [#for item in data.education { entry(item) }]) }
#context [#metadata(counter(page).final().first()) <harness-pages>]
