The Memory extension turns biorouter into a knowledgeable assistant by allowing you to teach it personalized key information (e.g. commands, code snippets, preferences and configurations) that it can recall and apply later. Whether it’s project-specific (local) or universal (global) knowledge, biorouter learns and remembers what matters most to you.

This tutorial covers enabling and using the Memory MCP Server, which is a built-in biorouter extension.  

## Configuration

  
  
  
  

 
  1. Run the `configure` command:
  ```sh
  biorouter configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   biorouter-configure 
  │
  ◇  What would you like to configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● memory
  // highlight-end
  |
  └  Extension settings updated successfully
  ```
  

## Why Use Memory?  
With the Memory extension, you’re not just storing static notes, you’re teaching biorouter how to assist you better. Imagine telling biorouter:  

> _learn everything about MCP servers and save it to memory._

Later, you can ask:
> _utilizing our MCP server knowledge help me build an MCP server._ 

biorouter will recall everything you’ve saved as long as you instruct it to remember. This makes it easier to have consistent results when working with biorouter.

biorouter loads all saved memories at the start of a session and includes them in every prompt sent to the LLM. For large or detailed instructions, store them in files and instruct biorouter to reference those files:

> _Remember that if I ask for help writing JavaScript, I want you to refer to "/path/to/javascript_notes.txt" and follow the instructions in that file._

## Trigger Words and When to Use Them
biorouter also recognizes certain trigger words that signal when to store, retrieve, or remove memory.

| **Trigger Words**   | **When to Use** |
|---------------------|----------------|
| remember            | Store useful info for later use |
| forget           | Remove a stored memory |
| memory           | General memory-related actions |
| save             | Save a command, config, or preference |
| remove memory    | Delete specific stored data |
| clear memory     | Wipe all stored memories |
| search memory    | Find previously stored data |
| find memory      | Locate specific saved information |

## Example Usage

In this example, I’ll show you how to make biorouter a knowledgeable research assistant by teaching it about your lab’s data-analysis standards. With the Memory extension, biorouter can store structured information and recall it when needed, helping you stay consistent in your work.

This means you no longer have to repeat yourself. biorouter will remember your project’s requirements and automatically apply them to new tasks.

> **Tip:** If you frequently work with analysis standards or other structured knowledge, biorouter may automatically suggest saving them to memory for you when it detects a pattern in your workflow.

  
   1. Open a new session in biorouter Desktop
  
  

  1. Open a terminal and start a new biorouter session:

  ```sh
  biorouter session
  ```

  

### Step 1: Teach biorouter Your Analysis Standards
    #### biorouter Prompt #1
    ```
    biorouter, learn our cohort data-analysis standards:

    - All cohorts must be defined with explicit OMOP concept IDs
    - Continuous variables are reported as mean ± SD; categorical as n (%)
    - Multiple-comparison correction uses Benjamini-Hochberg FDR
    - Patient identifiers must be de-identified before analysis
    - Every analysis needs a reproducible script and a methods summary

    Remember this so you can help me follow these practices.
    ```

    #### biorouter Output

    ```
    I've learned your analysis standards. I'll help enforce these requirements when working on cohort analyses.

    Storing in:
        - Category: analysis_standards
        - Tags: #cohort #omop #statistics #reproducibility
        - Scope: global (these are general standards)

    I can now help with:
        - Cohort definition with OMOP concepts
        - Descriptive and inferential statistics
        - Multiple-comparison correction
        - De-identification and methods reporting
    ```
### Step 2: Use Stored Knowledge to Run a New Cohort Analysis
    #### biorouter Prompt # 2

    ```
    Help me compare baseline characteristics between the treatment and control cohorts
    ```
    #### biorouter Output
    ```
    I see you need a baseline comparison. Based on our cohort data-analysis standards that I have stored in memory, I'll ensure it meets all our requirements.

    [.. Checking stored analysis standards ...]

    I've followed all our required elements:
    
    ✅ Cohorts defined via OMOP concept IDs
    ✅ Mean ± SD for continuous, n (%) for categorical
    ✅ Benjamini-Hochberg FDR correction
    ✅ De-identified patient data
    ✅ Reproducible script + methods summary

    Here's your compliant analysis script:

    # baseline_comparison.py
    import pandas as pd
    from scipy import stats
    from statsmodels.stats.multitest import multipletests

    treatment = load_cohort(concept_ids=[201826])   # OMOP concept
    control = load_cohort(concept_ids=[4193704])

    # Continuous: mean ± SD, two-sample t-test
    # Categorical: n (%), chi-square test
    pvals = run_baseline_tests(treatment, control)

    # Benjamini-Hochberg FDR correction per our standard
    _, qvals, _, _ = multipletests(pvals, method="fdr_bh")

    Would you like me to run the full comparison and generate the Table 1 summary?
    ```
