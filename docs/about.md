<!-- 

NOTE: the parts in { } are variables that are replaced as the `xmlhub`
program is running (see function `markdown_with_variables_to_html` for
which variables are supported).

-->

{{#if public}}

# About XML Hub

XML Hub is an initiative developed by [cEvo](https://bsse.ethz.ch/cevo) in collaboration with the [Taming the BEAST team](https://taming-the-beast.org/).

## Contact

You can contact us at:

Christian Jaeger [`<ch@christianjaeger.ch>`](mailto:ch@christianjaeger.ch)

Marcus Overwater [`<moverwater@ethz.ch>`](mailto:moverwater@ethz.ch)

{{else}}

# About

## About this locally-installed program

{versionAndBuildInfo}

The source code for this program is in the {xmlhubIndexerRepoLink}
repository, pre-compiled binaries are in the
{xmlhubIndexerBinariesRepoLink} repository.

## Your XML Hub instance

* cEvo: {xmlhubRepoLink}

## Your XML Hub maintainer

* cEvo: Marcus Overwater [`<moverwater@ethz.ch>`](mailto:moverwater@ethz.ch)

{{/if}}
