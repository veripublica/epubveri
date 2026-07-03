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

  <pattern id="opf-refines-must-be-relative">
    <rule context="*[@refines]">
      <assert test="not(contains(@refines, '://'))"
        >@refines must be a relative URL</assert>
    </rule>
  </pattern>

  <!-- "should use a fragment identifier" is a warning with its own code
       (RSC-017), not RSC-005 - the schematron::run() call site in opf.rs
       maps every finding to RSC-005/Error uniformly, so this is
       hand-coded in opf.rs instead (same reason OPF-086's rendition:
       spread-portrait check is hand-coded, not Schematron). -->

  <!-- 5.4 The package element -->
  <!-- "metadata must come before manifest" needs sibling-order comparison,
       which our XPath 1.0 core doesn't support (no preceding-sibling::
       axis) - hand-coded in opf.rs instead (a plain child-index compare). -->

  <!-- 5.5.2 Metadata values -->

  <pattern id="opf-identifier-not-empty">
    <rule context="dc:identifier">
      <assert test="string-length(normalize-space(.)) &gt; 0"
        >dc:identifier must be a string with length at least 1</assert>
    </rule>
  </pattern>

  <pattern id="opf-language-not-empty">
    <rule context="dc:language">
      <assert test="string-length(normalize-space(.)) &gt; 0"
        >dc:language must be a string with length at least 1</assert>
    </rule>
  </pattern>

  <!-- EPUB 3 only: an empty dc:title is an error (RSC-005). EPUB 2 is
       more lenient here - a real corpus fixture expects only a warning
       (OPF-055, hand-coded in opf.rs since this schema engine reports
       every finding as RSC-005 uniformly). -->
  <pattern id="opf-title-not-empty">
    <rule context="opf:package[starts-with(@version, '3')]/opf:metadata/dc:title">
      <assert test="string-length(normalize-space(.)) &gt; 0"
        >dc:title must be a string with length at least 1</assert>
    </rule>
  </pattern>

  <pattern id="opf-meta-value-not-empty">
    <rule context="opf:meta[@property]">
      <assert test="string-length(normalize-space(.)) &gt; 0"
        >a meta element's value must be a string with length at least 1</assert>
    </rule>
  </pattern>

  <!-- 5.5.4.4 The dc:date element -->

  <pattern id="opf-date-cardinality">
    <rule context="opf:metadata[dc:date]">
      <assert test="count(dc:date) = 1"
        >element "dc:date" not allowed here (only one dc:date element is allowed)</assert>
    </rule>
  </pattern>

  <!-- 5.5.5 The meta element -->

  <pattern id="opf-meta-property-not-empty">
    <rule context="opf:meta">
      <assert test="string-length(normalize-space(@property)) &gt; 0"
        >value of attribute "property" is invalid (must not be empty)</assert>
    </rule>
  </pattern>

  <pattern id="opf-meta-property-single-token">
    <rule context="opf:meta[@property]">
      <assert test="not(contains(normalize-space(@property), ' '))"
        >only one value must be specified for the "property" attribute</assert>
    </rule>
  </pattern>

  <pattern id="opf-meta-scheme-single-token">
    <rule context="opf:meta[@scheme]">
      <assert test="not(contains(normalize-space(@scheme), ' '))"
        >only one value must be specified for the "scheme" attribute</assert>
    </rule>
  </pattern>

  <!-- an unprefixed, unknown "scheme" value is OPF-027, not RSC-005 -
       hand-coded in opf.rs for the same reason as above. -->

  <!-- 5.5.7 The link element -->

  <pattern id="opf-link-properties-not-empty">
    <rule context="opf:link[@properties]">
      <assert test="string-length(normalize-space(@properties)) &gt; 0"
        >value of attribute "properties" is invalid (must not be empty)</assert>
    </rule>
  </pattern>

  <!-- 5.8 Collections -->

  <pattern id="opf-collection-manifest-not-toplevel">
    <rule context="opf:package/opf:collection[normalize-space(@role) = 'manifest']">
      <assert test="false()"
        >a manifest collection must be the child of another collection</assert>
    </rule>
  </pattern>

  <!-- 5.9.3 NCX -->

  <pattern id="opf-legacy-ncx-toc-required">
    <rule context="opf:package[opf:manifest/opf:item[normalize-space(@media-type) = 'application/x-dtbncx+xml']]/opf:spine[not(@toc)]">
      <assert test="false()"
        >the spine "toc" attribute must be set when an NCX document is present</assert>
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

  <!-- D.3 Meta properties vocabulary — refines-target-type and cardinality
       rules for the fixed set of meta[@property] values the EPUB 3
       "Meta properties vocabulary" defines. Each property refines either a
       plain Dublin Core element (checked via a namespace-qualified
       existence test, e.g. //dc:subject[@id=...] — our XPath 1.0 core has
       no local-name() function) or another meta property (checked by the
       target's own @property) — a @refines value may or may not have a
       leading '#' in the wild, so every target lookup strips one optional
       leading '#' before resolving @id. -->

  <pattern id="meta-authority-must-refine-subject">
    <rule context="opf:meta[normalize-space(@property) = 'authority']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and //dc:subject[normalize-space(@id) = $target-id]"
        >Property "authority" must refine a "subject" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-authority-needs-term">
    <rule context="opf:meta[normalize-space(@property) = 'authority']">
      <assert test="//opf:meta[normalize-space(@property) = 'term'][normalize-space(@refines) = normalize-space(current()/@refines)]"
        >A term property must be associated with each authority property</assert>
    </rule>
  </pattern>

  <pattern id="meta-authority-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'authority']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'authority'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >Only one pair of authority and term properties may be defined for the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-term-must-refine-subject">
    <rule context="opf:meta[normalize-space(@property) = 'term']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and //dc:subject[normalize-space(@id) = $target-id]"
        >Property "term" must refine a "subject" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-term-needs-authority">
    <rule context="opf:meta[normalize-space(@property) = 'term']">
      <assert test="//opf:meta[normalize-space(@property) = 'authority'][normalize-space(@refines) = normalize-space(current()/@refines)]"
        >An authority property must be associated with each term property</assert>
    </rule>
  </pattern>

  <pattern id="meta-term-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'term']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'term'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >Only one pair of authority and term properties may be defined for the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-belongs-to-collection-refines">
    <rule context="opf:meta[normalize-space(@property) = 'belongs-to-collection'][@refines]">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="//opf:meta[normalize-space(@id) = $target-id]/@property = 'belongs-to-collection'"
        >Property "belongs-to-collection" can only refine other "belongs-to-collection" properties</assert>
    </rule>
  </pattern>

  <pattern id="meta-collection-type-must-refine">
    <rule context="opf:meta[normalize-space(@property) = 'collection-type']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and //opf:meta[normalize-space(@id) = $target-id]/@property = 'belongs-to-collection'"
        >Property "collection-type" must refine a "belongs-to-collection" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-collection-type-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'collection-type']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'collection-type'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"collection-type" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-display-seq-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'display-seq']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'display-seq'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"display-seq" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-file-as-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'file-as']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'file-as'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"file-as" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-group-position-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'group-position']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'group-position'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"group-position" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-identifier-type-must-refine">
    <rule context="opf:meta[normalize-space(@property) = 'identifier-type']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and (//dc:identifier[normalize-space(@id) = $target-id] or //dc:source[normalize-space(@id) = $target-id])"
        >Property "identifier-type" must refine an "identifier" or "source" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-identifier-type-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'identifier-type']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'identifier-type'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"identifier-type" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-role-must-refine">
    <rule context="opf:meta[normalize-space(@property) = 'role']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and (//dc:creator[normalize-space(@id) = $target-id] or //dc:contributor[normalize-space(@id) = $target-id] or //dc:publisher[normalize-space(@id) = $target-id])"
        >"role" must refine a "creator", "contributor", or "publisher" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-source-of-value">
    <rule context="opf:meta[normalize-space(@property) = 'source-of']">
      <assert test="normalize-space(.) = 'pagination'"
        >The "source-of" property must have the value "pagination"</assert>
    </rule>
  </pattern>

  <pattern id="meta-source-of-must-refine-source">
    <rule context="opf:meta[normalize-space(@property) = 'source-of']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and //dc:source[normalize-space(@id) = $target-id]"
        >The "source-of" property must refine a "source" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-source-of-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'source-of']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'source-of'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"source-of" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <pattern id="meta-title-type-must-refine">
    <rule context="opf:meta[normalize-space(@property) = 'title-type']">
      <let name="target-id" value="substring(normalize-space(@refines), 1 + starts-with(normalize-space(@refines), '#'))"/>
      <assert test="@refines and //dc:title[normalize-space(@id) = $target-id]"
        >Property "title-type" must refine a "title" property</assert>
    </rule>
  </pattern>

  <pattern id="meta-title-type-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'title-type']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'title-type'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >"title-type" cannot be declared more than once to refine the same expression</assert>
    </rule>
  </pattern>

  <!-- D.4 Metadata link vocabulary -->

  <pattern id="link-record-no-refines">
    <rule context="opf:link[contains(concat(' ', normalize-space(@rel), ' '), ' record ')]">
      <assert test="not(@refines)"
        >a "record" link must not have a "refines" attribute</assert>
    </rule>
  </pattern>

  <pattern id="link-voicing-needs-refines">
    <rule context="opf:link[contains(concat(' ', normalize-space(@rel), ' '), ' voicing ')]">
      <assert test="@refines"
        >a "voicing" link must have a "refines" attribute</assert>
    </rule>
  </pattern>

  <!-- D.8 Media Overlays vocabulary — active-class/playback-active-class
       cardinality (single-class-name and no-refines are already checked
       above, from the CSS-029/030 increment). -->

  <pattern id="media-active-class-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'media:active-class']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'media:active-class'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >the "media:active-class" property must not occur more than one time</assert>
    </rule>
  </pattern>

  <pattern id="media-playback-active-class-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'media:playback-active-class']">
      <assert test="count(//opf:meta[normalize-space(@property) = 'media:playback-active-class'][normalize-space(@refines) = normalize-space(current()/@refines)]) = 1"
        >the "media:playback-active-class" property must not occur more than one time</assert>
    </rule>
  </pattern>

  <!-- 8.2/8.3 Layout Rendering Control — rendition:layout/orientation/
       spread/flow are always package-global properties (never used with
       @refines, unlike the D.3 metadata-refinement properties above), each
       with a closed enum of valid values and a "declared only once"
       cardinality rule. Confirmed via real corpus fixtures
       (epub3/08-layout/files/rendition-*-global-*.opf). -->

  <pattern id="rendition-layout-value">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:layout']">
      <assert test="normalize-space(.) = 'reflowable' or normalize-space(.) = 'pre-paginated'"
        >The value of the "rendition:layout" property must be "reflowable" or "pre-paginated"</assert>
    </rule>
  </pattern>

  <pattern id="rendition-layout-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:layout'][not(@refines)]">
      <assert test="count(//opf:meta[normalize-space(@property) = 'rendition:layout'][not(@refines)]) = 1"
        >The "rendition:layout" property must not occur more than one time as a global value</assert>
    </rule>
  </pattern>

  <pattern id="rendition-layout-no-refines">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:layout']">
      <assert test="not(@refines)"
        >the "rendition:layout" property must not be used with a "refines" attribute</assert>
    </rule>
  </pattern>

  <pattern id="rendition-orientation-value">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:orientation']">
      <assert test="normalize-space(.) = 'auto' or normalize-space(.) = 'landscape' or normalize-space(.) = 'portrait'"
        >The value of the "rendition:orientation" property must be "auto", "landscape", or "portrait"</assert>
    </rule>
  </pattern>

  <pattern id="rendition-orientation-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:orientation'][not(@refines)]">
      <assert test="count(//opf:meta[normalize-space(@property) = 'rendition:orientation'][not(@refines)]) = 1"
        >The "rendition:orientation" property must not occur more than one time as a global value</assert>
    </rule>
  </pattern>

  <pattern id="rendition-orientation-no-refines">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:orientation']">
      <assert test="not(@refines)"
        >the "rendition:orientation" property must not be used with a "refines" attribute</assert>
    </rule>
  </pattern>

  <pattern id="rendition-spread-value">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:spread']">
      <assert test="normalize-space(.) = 'auto' or normalize-space(.) = 'none' or normalize-space(.) = 'landscape' or normalize-space(.) = 'portrait' or normalize-space(.) = 'both'"
        >The value of the "rendition:spread" property must be "auto", "none", "landscape", "portrait", or "both"</assert>
    </rule>
  </pattern>

  <pattern id="rendition-spread-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:spread'][not(@refines)]">
      <assert test="count(//opf:meta[normalize-space(@property) = 'rendition:spread'][not(@refines)]) = 1"
        >The "rendition:spread" property must not occur more than one time as a global value</assert>
    </rule>
  </pattern>

  <pattern id="rendition-spread-no-refines">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:spread']">
      <assert test="not(@refines)"
        >the "rendition:spread" property must not be used with a "refines" attribute</assert>
    </rule>
  </pattern>

  <pattern id="rendition-flow-value">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:flow']">
      <assert test="normalize-space(.) = 'auto' or normalize-space(.) = 'paginated' or normalize-space(.) = 'scrolled-continuous' or normalize-space(.) = 'scrolled-doc'"
        >The value of the "rendition:flow" property must be "auto", "paginated", "scrolled-continuous", or "scrolled-doc"</assert>
    </rule>
  </pattern>

  <pattern id="rendition-flow-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:flow'][not(@refines)]">
      <assert test="count(//opf:meta[normalize-space(@property) = 'rendition:flow'][not(@refines)]) = 1"
        >The "rendition:flow" property must not occur more than one time as a global value</assert>
    </rule>
  </pattern>

  <pattern id="rendition-flow-no-refines">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:flow']">
      <assert test="not(@refines)"
        >the "rendition:flow" property must not be used with a "refines" attribute</assert>
    </rule>
  </pattern>

  <!-- rendition:viewport (deprecated) cardinality; the deprecated-usage
       warning (OPF-086, every occurrence) and syntax check are hand-coded
       in opf.rs since they reuse the layout.rs viewport-grammar parser. -->
  <pattern id="rendition-viewport-cardinality">
    <rule context="opf:meta[normalize-space(@property) = 'rendition:viewport'][not(@refines)]">
      <assert test="count(//opf:meta[normalize-space(@property) = 'rendition:viewport'][not(@refines)]) = 1"
        >The "rendition:viewport" property must not occur more than one time as a global value</assert>
    </rule>
  </pattern>
</schema>
