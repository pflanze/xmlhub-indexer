#!/usr/bin/env python3

import os, sys, re, time
from fnmatch import fnmatch
from optparse import OptionParser


usage = "usage: %prog [option]"
parser = OptionParser(usage=usage)

parser.add_option("-i","--in",
                  dest = "input",
                  default = ".",
                  metavar = "path",
                  help = "Path to input files or a single input XML file [required]")

parser.add_option("-p","--pattern",
                  dest = "pattern",
                  default = "*.xml",
                  metavar = "",
                  help = "Pattern to match for XML files if only an input path is provided [default = %default]")

parser.add_option("-o","--out",
                  dest = "output",
                  default = ".",
                  metavar = "path",
                  help = "Path to save output file(s) in [required]")

parser.add_option("-m","--message",
                  dest = "message",
                  default = "SEQUENCES REMOVED TO COMPLY WITH GISAID TERMS OF USE",
                  metavar = "path",
                  help = "Message to add to start of XML file [optional]")

(options,args) = parser.parse_args()

################################################################################################################################  

def seqRepl(matchobj):
      (prefix, seq, postfix) = matchobj.groups()
      return (prefix + "-" + postfix)
#


################################################################################################################################  
start = time.time()

# Process input and output pars
if (not os.path.exists(options.input)):
      sys.stderr.write("Input path/file '%s' does not exist! Exiting...\n" % options.input)
      sys.exit()

if (os.path.isdir(options.input)):
      inputpath = os.path.abspath(options.input)+"/"
      pattern   = options.pattern

elif (os.path.isfile(options.input)):
      inputpath = os.path.abspath(options.input)
      pattern   = inputpath[inputpath.rfind("/")+1:]
      inputpath = inputpath[:inputpath.rfind("/")]+"/"      
else:
      sys.stderr.write("Statement should be unreachable. Something wrong with input path/file '%s'. Exiting...\n" % options.input)
      sys.exit()

# Make output folder
outputpath = os.path.abspath(options.output)+"/"
if (not os.path.exists(outputpath)):
    os.mkdir(outputpath)


# Iterate through files
for filename in sorted(os.listdir(inputpath)):
      if (fnmatch(filename, pattern)):
            sys.stdout.write(filename+":")


            xml = open(inputpath+filename, 'r').read()

            (newxml, n) = re.subn(r'(<sequence.*?value\s*?=\s*?")(.*?)(")', seqRepl, xml, flags=re.DOTALL)
            (newxml, m) = re.subn(r'<data', "<!-- "+options.message+" -->\n\t<data", newxml, flags=re.DOTALL)

            sys.stdout.write("\t%d sequences blinded\n" % n)
            
            outfile = open(outputpath + filename[:filename.rfind(".")]+".blinded.xml", 'w')
            #outfile.write("<!-- %s -->\n" % options.message)
            outfile.write(newxml)
            outfile.close()

      #
#

end = time.time()
sys.stdout.write("Total time taken: "+str(end-start)+" seconds\n")

