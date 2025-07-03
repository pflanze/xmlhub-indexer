# Contributing to XML Hub

## Browsing the files

There is an [index over all the XML files](https://cevo-git.ethz.ch/cevo-resources/xmlhub). This is the latest published version of the index, representing the latest published documents. If you are working locally, you can use the [xmlhub tool](tool.html) to generate an index for the local files.

You can also search the contents of all files via the GitLab search
form, which you can find towards the top left corner of this page (the
input field saying "Search or go to...").

## When to Add Your XML Files

New XML files are welcome to be uploaded at any stage in a project's development, although ideally it should be verified that they run (at the point of upload, with the intended data). Version updates may later render this false, in which case the XML should serve as a pointer to which packages might be relevant to a specific analysis. Information about when to update files can be seen below (in Versioning and Backup).

## How to Add Your XML Files

### Create a folder

A new folder should be added for each project, with an explanatory name. Within this folder, each component XML file should be added, with a name briefly describing the aim of the specific BEAST2 analysis, highlighting particular elements which might be more unique and therefore of particular use to other users. It's possible to add nested sub-folders if deemed useful. 

While not required, it would also be very useful for browsing and discovery to add a `README.md` file for each folder, with a brief description of the project and where it is published (if it is). This could be as simple as: 

```
# <Project name>

<Brief description of the aims of the analyses in the folder.>

These analyses form part of <authors>, [<Paper title>](https://dx.doi.org/<doi>), <journal>, <year>.

```

replacing placeholders between `< >` with content. 

### Add metadata to files

To help other users find relevant files, before uploading your files to the hub, please add a header to each XML file, between `<?xml..>` (XML declaration) and `<beast ..` (the first XML opening tag), using the template below. Please try to fill out as many of the fields as possible and leave the others marked `NA`. The keyword section is particularly important, to help other users find XML files that are useful for them. Further, the contact person section is essential, indicating the cEvo member who should be contacted if someone wants to learn more about a particular XML file.

You can copy a template of the required attributes from the [Attributes list](attributes.html) page. Alternatively, you can use the [xmlhub tool](tool.html) to automatically add the attributes to your XML files when you add them to your local repository.

Please add the minor version of BEAST used as far as possible, since the XML metadata only contains the major version (e.g. 2.6) and only if it was created by BEAUti. Please also add package versions as far as possible (give the package name followed by space then the version number)! 

The indexer program treats any amount of any kind of whitespace
(spaces and newlines) the same for the indices, although it preserves
line breaks and tabs for attributes that are not lists (like comments
and description), so you're free to choose. But it's important to put
every attribute into a separate `<!--` ... `-->` pair.

See [attributes](attributes.html) for how each attribute is processed
exactly.

A full (mostly fictional) example:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!--Keywords: birth-death, extinct, skyline -->
<!-- Version: BEAST 2.7.1 -->
<!-- Packages: base, BDSKY 1.5.1,
         SA 2.1.1 -->
<!--Description: Inference of birth, death, and fossil sampling rates
   using a fixed dinosaur phylogeny (read from file), specifying 
   change times for evolutionary rates. -->
<!--Comments: Uses BDMMPrime as tree is fully extinct, so requires
   model combining BDSKY with capability of a final sample offset
   (fso), enabling all tips to shift in time. Age constraints for 
   each tip are specified from an accompanying csv file. Trees are
   logged, as the topology does not change but the inferred branch 
   lengths and tip ages do. -->
<!--
   Citation: Allen BJ, Volkova Oliveira MV, Stadler T, Vaughan TG,
   Warnock RCM. Mechanistic phylodynamic models do not provide 
   conclusive evidence that non-avian dinosaurs were in decline 
   before their final extinction. Cambridge Prisms: Extinction, 2, 
   e6.
-->
<!--DOI: 10.1234/ext.2024.5  -->
<!--Contact:  Emily Example, Leslie Lilliput -->

<beast version="2.7" namespace="beast.core
...
```

### Private Data

It is not essential to provide working data within the XML, particularly if the analysis was designed using private data. Currently the XML hub is on our local ETH server and is thus fully private and secure, however this might change in future (see Future Ideas). Therefore, if your XML files contain any restricted data, please remove the data prior to upload and add a comment in the XML that the data has been removed. For example:

```xml
<!--Data removed for privacy reasons. -->
```

If you wish to be more helpful to your fellow users, you could also replace the data with "dummy" data, indicating the required data format (and potentially even allowing the BEAST2 analysis to run).

If your XML file contains a lot of data, or you have a lot of XML files to add, you can use the [xmlhub tool](tool.html) or the [Blinder scripts](blinder-scripts.html) page to "blind" your file. These scripts will replace the genetic sequences with a dummy all-gap alignment while keeping everything else (e.g. tip-dates) intact so the XML file still runs. If your XML file contains other non-sequence private data these scripts are not applicable! Keep in mind that `beast2blinder.py` and `beast1blinder.py` were written for BEAST 2.6 and BEAST 1.10, respectively, and have not been tested on other versions!

### Versioning and Backup

Having the XML hub on a git repo allows version tracking. There is also a mirror of the XML hub on our group server, which has the same backup policy as all other folders in the `d.ethz.ch` server. 

If you would like to update an XML file on the hub and think both the new and old version of the file are relevant for users, you can add a new version to the project folder with version numbers in the file name. Please also update the `comments` section of both files for transparency.

## Sharing
Everyone in cEvo can get an account on the cEvo GitLab and should then be able to access the repository. The repository can also be accessed via the group server. It is possible to create guest accounts on our GitLab server for external collaborators, however this should be used sparingly. This is a cEvo internal resource and **NOT** a public resource for all BEAST2 users!

## Future Ideas

In future we might like to move the XML hub to a (private) github repo. This would make it easier to share files with external collaborators. However, as data on github is not stored on ETH servers this means we are restricted from uploading private data and should ideally (to prevent any unplanned mishaps) not store any data that cannot be made public.
