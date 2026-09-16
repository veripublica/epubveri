<?xml version="1.0" encoding="UTF-8"?>
<!--
  epubveri — EPUB 3 XHTML content-model Schematron rules.

  Authored from scratch for epubveri (copyright the project owner; ships
  under the project's dual AGPL/commercial license). NOT derived from
  epubcheck's real Schematron files, which were read only for understanding
  during design — same clean-room stance as schemas/package.sch and
  schemas/xhtml.rng.

  These are the HTML5 content-model constraints a RELAX NG grammar cannot
  express — ancestor/descendant nesting rules and attribute-level structural
  rules — so schemas/xhtml.rng handles element/text placement and this handles
  the rest. (idref/idrefs *resolution* is hand-coded in htm.rs, since checking
  every whitespace-separated token needs iteration the XPath 1.0 core lacks.)
  EPUB 3 only. epubcheck reports nearly every violation here as RSC-005 and we
  match that, so RSC-005/error is what a pattern gets by default. A pattern that
  needs another id or severity says so on its own assert/report, with
  Schematron's `flag` (the message id) and `role` (the severity) — which is how
  the `obsolete-*` patterns below report USAGE RSC-036.
-->
<schema xmlns="http://purl.oclc.org/dsdl/schematron">
  <ns uri="http://www.w3.org/1999/xhtml" prefix="h"/>
  <ns uri="http://www.w3.org/2001/10/synthesis" prefix="ssml"/>

  <!-- No interactive content nested inside interactive content (a, button). -->
  <pattern id="no-interactive-in-a--a">
    <rule context="h:a">
      <report test="ancestor::h:a">interactive content (a "a" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "a" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--audio-controls">
    <rule context="h:audio[@controls]">
      <report test="ancestor::h:a">interactive content (a "audio (with controls)" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "audio (with controls)" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--button">
    <rule context="h:button">
      <report test="ancestor::h:a">interactive content (a "button" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "button" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--details">
    <rule context="h:details">
      <report test="ancestor::h:a">interactive content (a "details" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "details" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--embed">
    <rule context="h:embed">
      <report test="ancestor::h:a">interactive content (a "embed" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "embed" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--iframe">
    <rule context="h:iframe">
      <report test="ancestor::h:a">interactive content (a "iframe" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "iframe" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--img-usemap">
    <rule context="h:img[@usemap]">
      <report test="ancestor::h:a">interactive content (a "img (with usemap)" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "img (with usemap)" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--input-nottype-hidden">
    <rule context="h:input[not(@type='hidden')]">
      <report test="ancestor::h:a">interactive content (a "input" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "input" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--label">
    <rule context="h:label">
      <report test="ancestor::h:a">interactive content (a "label" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "label" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--menu">
    <rule context="h:menu">
      <report test="ancestor::h:a">interactive content (a "menu" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "menu" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--object-usemap">
    <rule context="h:object[@usemap]">
      <report test="ancestor::h:a">interactive content (a "object (with usemap)" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "object (with usemap)" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--select">
    <rule context="h:select">
      <report test="ancestor::h:a">interactive content (a "select" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "select" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--textarea">
    <rule context="h:textarea">
      <report test="ancestor::h:a">interactive content (a "textarea" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "textarea" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>
  <pattern id="no-interactive-in-a--video-controls">
    <rule context="h:video[@controls]">
      <report test="ancestor::h:a">interactive content (a "video (with controls)" element) must not appear inside an "a" element</report>
      <report test="ancestor::h:button">interactive content (a "video (with controls)" element) must not appear inside a "button" element</report>
    </rule>
  </pattern>

  <!-- Elements that must not be nested inside a given ancestor. -->
  <pattern id="no-audio-in-audio">
    <rule context="h:audio">
      <report test="ancestor::h:audio">a "audio" element must not appear inside a "audio" element</report>
    </rule>
  </pattern>
  <pattern id="no-video-in-audio">
    <rule context="h:video">
      <report test="ancestor::h:audio">a "video" element must not appear inside a "audio" element</report>
    </rule>
  </pattern>
  <pattern id="no-video-in-video">
    <rule context="h:video">
      <report test="ancestor::h:video">a "video" element must not appear inside a "video" element</report>
    </rule>
  </pattern>
  <pattern id="no-audio-in-video">
    <rule context="h:audio">
      <report test="ancestor::h:video">a "audio" element must not appear inside a "video" element</report>
    </rule>
  </pattern>
  <pattern id="no-address-in-address">
    <rule context="h:address">
      <report test="ancestor::h:address">a "address" element must not appear inside a "address" element</report>
    </rule>
  </pattern>
  <pattern id="no-header-in-address">
    <rule context="h:header">
      <report test="ancestor::h:address">a "header" element must not appear inside a "address" element</report>
    </rule>
  </pattern>
  <pattern id="no-footer-in-address">
    <rule context="h:footer">
      <report test="ancestor::h:address">a "footer" element must not appear inside a "address" element</report>
    </rule>
  </pattern>
  <pattern id="no-form-in-form">
    <rule context="h:form">
      <report test="ancestor::h:form">a "form" element must not appear inside a "form" element</report>
    </rule>
  </pattern>
  <pattern id="no-progress-in-progress">
    <rule context="h:progress">
      <report test="ancestor::h:progress">a "progress" element must not appear inside a "progress" element</report>
    </rule>
  </pattern>
  <pattern id="no-meter-in-meter">
    <rule context="h:meter">
      <report test="ancestor::h:meter">a "meter" element must not appear inside a "meter" element</report>
    </rule>
  </pattern>
  <!-- epubcheck's `bdo-dir` (`required-attr` with elem=h:bdo, attr=dir).
       HTML5 makes `dir` required on `bdo` — the element exists to override
       the bidi direction, so without one it says nothing. A real gap here,
       found by diffing `schematron-error.xhtml` finding by finding rather
       than by count: the totals differed by two and one of the two was an
       over-report of ours, which cancelled half of it. -->
  <pattern id="bdo-dir-required">
    <rule context="h:bdo">
      <assert test="@dir">a "bdo" element must have a "dir" attribute</assert>
    </rule>
  </pattern>
  <pattern id="no-dfn-in-dfn">
    <rule context="h:dfn">
      <report test="ancestor::h:dfn">a "dfn" element must not appear inside a "dfn" element</report>
    </rule>
  </pattern>
  <pattern id="no-table-in-caption">
    <rule context="h:table">
      <report test="ancestor::h:caption">a "table" element must not appear inside a "caption" element</report>
    </rule>
  </pattern>
  <pattern id="no-header-in-header">
    <rule context="h:header">
      <report test="ancestor::h:header">a "header" element must not appear inside a "header" element</report>
    </rule>
  </pattern>
  <pattern id="no-footer-in-header">
    <rule context="h:footer">
      <report test="ancestor::h:header">a "footer" element must not appear inside a "header" element</report>
    </rule>
  </pattern>
  <pattern id="no-footer-in-footer">
    <rule context="h:footer">
      <report test="ancestor::h:footer">a "footer" element must not appear inside a "footer" element</report>
    </rule>
  </pattern>
  <pattern id="no-header-in-footer">
    <rule context="h:header">
      <report test="ancestor::h:footer">a "header" element must not appear inside a "footer" element</report>
    </rule>
  </pattern>
  <pattern id="no-label-in-label">
    <rule context="h:label">
      <report test="ancestor::h:label">a "label" element must not appear inside a "label" element</report>
    </rule>
  </pattern>

  <!-- Elements that must have a given ancestor. -->
  <pattern id="required-ancestor--area">
    <rule context="h:area">
      <assert test="ancestor::h:map">an "area" element must appear inside a "map" element</assert>
    </rule>
  </pattern>
  <pattern id="required-ancestor--img-ismap">
    <rule context="h:img[@ismap]">
      <assert test="ancestor::h:a[@href]">an "img (with ismap)" element must appear inside a "a (with href)" element</assert>
    </rule>
  </pattern>

  <!-- Attribute-level content-model constraints (group A). Structural rules a
       grammar can't state; idref/idrefs *resolution* is hand-coded in htm.rs
       (it needs per-token iteration the XPath 1.0 core can't express). -->

  <pattern id="map-name-unique">
    <rule context="h:map[@name]">
      <assert test="count(//h:map[@name = current()/@name]) = 1">duplicate map name "<value-of select="@name"/>"</assert>
    </rule>
  </pattern>
  <pattern id="map-id-equals-name">
    <rule context="h:map[@id and @name]">
      <assert test="@id = @name">a "map" element's "id" must equal its "name"</assert>
    </rule>
  </pattern>
  <pattern id="select-single-selected">
    <rule context="h:select[not(@multiple)]">
      <report test="count(descendant::h:option[@selected]) &gt; 1">a "select" without "multiple" must not have more than one selected "option"</report>
    </rule>
  </pattern>
  <pattern id="link-sizes-icon-only">
    <rule context="h:link[@sizes]">
      <assert test="@rel='icon'">the "sizes" attribute is only allowed on a "link" whose "rel" is "icon"</assert>
    </rule>
  </pattern>
  <pattern id="meta-charset-once">
    <rule context="h:meta[@charset]">
      <assert test="count(preceding-sibling::h:meta[@charset]) = 0">only one "meta" element with a "charset" attribute is allowed per document</assert>
    </rule>
  </pattern>
  <pattern id="ssml-ph-not-nested">
    <rule context="*[@ssml:ph]">
      <report test="ancestor::*[@ssml:ph]">the "ssml:ph" attribute must not appear on a descendant of an element that also carries it</report>
    </rule>
  </pattern>
  <!-- DPUB-ARIA's two page-running-head roles prohibit an accessible name.
       epubcheck states this in Schematron rather than the grammar, because
       its grammar admits the roles on anything taking any role at all
       (`epub-xhtml-integration.rnc`), so the constraint has nowhere else to
       live. `role` is a single token for us (see the ariaRole note in
       xhtml.rng), which is why this matches the whole attribute where
       epubcheck tokenises it. -->
  <pattern id="aria-role-name-prohibited">
    <rule context="h:*[@role = 'doc-pagefooter' or @role = 'doc-pageheader']">
      <report test="@aria-label">an element with the "<value-of select="@role"/>" role must not carry an "aria-label" attribute</report>
      <!-- Guarded on the absence of the other spelling so an element
           carrying both draws one finding, as epubcheck's single
           `@aria-label|@aria-labelledby` report does. -->
      <report test="@aria-labelledby and not(@aria-label)">an element with the "<value-of select="@role"/>" role must not carry an "aria-labelledby" attribute</report>
    </rule>
  </pattern>
  <!-- Obsolete but conforming HTML: reported as USAGE RSC-036 rather than
       the RSC-005 every other pattern here produces (epubcheck 5.4.0,
       `obsolete.*` in `epub-xhtml-30.sch`). The grammar admits these
       attributes only in the restricted forms xhtml.rng documents, so a rule
       firing here is always on a value the schema already accepted. -->
  <!-- A `script` element with a `src` must declare either `type="module"`
       or a JavaScript media type; anything else is a schema error. epubcheck
       5.4.0 states this in Schematron on its own admission — the constraint
       belongs to validator.nu's non-schema code, which it has not integrated
       (w3c/epubcheck 5911ef1). The sixteen media types are
       `OPFChecker.isScriptType`'s, mirrored in `opf.rs`'s
       `is_script_media_type`; `module` is the HTML keyword and is not one of
       them, which is why it is named separately here and there. -->
  <pattern id="script-src-type">
    <rule context="h:script[@src][@type]">
      <let name="t" value="lower-case(normalize-space(@type))"/>
      <report test="not($t = 'module' or $t = 'application/javascript' or $t = 'text/javascript' or $t = 'application/ecmascript' or $t = 'application/x-ecmascript' or $t = 'application/x-javascript' or $t = 'text/ecmascript' or $t = 'text/javascript1.0' or $t = 'text/javascript1.1' or $t = 'text/javascript1.2' or $t = 'text/javascript1.3' or $t = 'text/javascript1.4' or $t = 'text/javascript1.5' or $t = 'text/jscript' or $t = 'text/livescript' or $t = 'text/x-ecmascript' or $t = 'text/x-javascript')">a "script" element with a "src" must declare "module" or a JavaScript media type, not "<value-of select="@type"/>"</report>
    </rule>
  </pattern>
  <pattern id="obsolete-img-border">
    <rule context="h:img[@border]">
      <report test="true()" role="usage" flag="RSC-036">the "border" attribute on an "img" element is obsolete; use CSS instead</report>
    </rule>
  </pattern>
  <pattern id="obsolete-script-charset">
    <rule context="h:script[@charset]">
      <report test="true()" role="usage" flag="RSC-036">the "charset" attribute on a "script" element is obsolete</report>
    </rule>
  </pattern>
  <pattern id="obsolete-style-type">
    <rule context="h:style[@type]">
      <report test="true()" role="usage" flag="RSC-036">the "type" attribute on a "style" element is obsolete</report>
    </rule>
  </pattern>
  <pattern id="obsolete-a-name">
    <rule context="h:a[@name]">
      <report test="true()" role="usage" flag="RSC-036">the "name" attribute on an "a" element is obsolete; use an "id" on the nearest container instead</report>
    </rule>
  </pattern>
  <pattern id="track-rules">
    <rule context="h:track">
      <report test="@label and normalize-space(@label) = ''">a "track" element's "label" must not be empty</report>
      <report test="@default and preceding-sibling::h:track[@default]">only one "track" of a media element may have the "default" attribute</report>
    </rule>
  </pattern>
</schema>
