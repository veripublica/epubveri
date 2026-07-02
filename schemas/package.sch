<?xml version="1.0" encoding="UTF-8"?>
<!--
  epubveri — EPUB package-document Schematron rules.

  Authored from scratch for epubveri (copyright the project owner; ships
  under the project's dual AGPL/commercial license). NOT derived from
  epubcheck's real Schematron files, which were read only for understanding
  during this increment's design (see CLAUDE.md's dated notes) — same
  clean-room stance as schemas/package.rng and schemas/xhtml.rng.

  Evaluated with epubveri's own XPath 1.0 *core* subset engine (src/xpath/) —
  deliberately without matches()/tokenize()/resolve-uri() (regex/URI-
  resolution engines, out of scope for this increment). That means a few
  real epubcheck checks aren't reachable yet: dcterms:modified's exact
  date-format regex, and @refines-as-relative-URL. Those are left for a
  later increment once (if) the engine grows those functions.

  Covers: id uniqueness across the whole package document; unique-identifier
  resolving to a real dc:identifier; dcterms:modified occurring exactly
  once; @refines fragment targets existing.
-->
<schema xmlns="http://purl.oclc.org/dsdl/schematron">
  <ns uri="http://www.idpf.org/2007/opf" prefix="opf"/>
  <ns uri="http://purl.org/dc/elements/1.1/" prefix="dc"/>

  <let name="id-set" value="//*[@id]"/>

  <pattern id="id-unique">
    <rule context="*[@id]">
      <assert test="count($id-set[normalize-space(@id) = normalize-space(current()/@id)]) = 1"
        >duplicate id "<value-of select="normalize-space(@id)"/>"</assert>
    </rule>
  </pattern>

  <pattern id="opf-unique-identifier">
    <rule context="opf:package[@unique-identifier]">
      <assert
        test="/opf:package/opf:metadata/dc:identifier[normalize-space(@id) = normalize-space(current()/@unique-identifier)]"
        >package unique-identifier "<value-of select="normalize-space(@unique-identifier)"/>" does not
        resolve to a dc:identifier element</assert>
    </rule>
  </pattern>

  <!-- dcterms:modified is an EPUB 3 requirement only; EPUB 2 packages don't
       have (or need) it, so this is scoped to version="3.*" packages. -->
  <pattern id="opf-dcterms-modified-count">
    <rule context="opf:package[starts-with(@version, '3')]/opf:metadata">
      <assert test="count(opf:meta[normalize-space(@property) = 'dcterms:modified']) = 1"
        >package metadata must have exactly one dcterms:modified meta element</assert>
    </rule>
  </pattern>

  <pattern id="opf-refines-target-exists">
    <rule context="*[@refines][starts-with(normalize-space(@refines), '#')]">
      <let name="refines-target" value="substring(normalize-space(@refines), 2)"/>
      <assert test="//*[normalize-space(@id) = $refines-target]"
        >@refines target "<value-of select="$refines-target"/>" does not exist</assert>
    </rule>
  </pattern>

  <!-- media:active-class / media:playback-active-class name the CSS class
       a reading system applies to the currently active/playing
       media-overlay element - a single, unqualified class name, not
       refining anything else. -->
  <pattern id="media-active-class-no-refines">
    <rule context="opf:meta[normalize-space(@property) = 'media:active-class' or normalize-space(@property) = 'media:playback-active-class']">
      <assert test="not(@refines)"
        >the "<value-of select="normalize-space(@property)"/>" property must not have a refines attribute</assert>
    </rule>
  </pattern>

  <pattern id="media-active-class-single-name">
    <rule context="opf:meta[normalize-space(@property) = 'media:active-class' or normalize-space(@property) = 'media:playback-active-class']">
      <assert test="not(contains(normalize-space(.), ' '))"
        >the "<value-of select="normalize-space(@property)"/>" property must define a single class name</assert>
    </rule>
  </pattern>
</schema>
