<?xml version="1.0" encoding="UTF-8"?>
<!--
  epubveri — EPUB 3 XHTML content-model Schematron rules.

  Authored from scratch for epubveri (copyright the project owner; ships
  under the project's dual AGPL/commercial license). NOT derived from
  epubcheck's real Schematron files, which were read only for understanding
  during design — same clean-room stance as schemas/package.sch and
  schemas/xhtml.rng.

  These are the HTML5 content-model constraints a RELAX NG grammar cannot
  express (a node must / must not have a given ancestor), so schemas/xhtml.rng
  handles element/text placement and this handles nesting. EPUB 3 only.
  Every violation is reported by epubcheck as RSC-005; we match that.
-->
<schema xmlns="http://purl.oclc.org/dsdl/schematron">
  <ns uri="http://www.w3.org/1999/xhtml" prefix="h"/>

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
</schema>
