from scripts.calculate_maximized_chunk_size import *
from scripts.convertors.keys_convertor import *
import concurrent.futures
import scripts.globals_vars
import re


def parse_MoNA_peak_list(peak_list_string):
    """
    Parse a peak list string from the MoNA database into a list of peaks.

    :param peak_list_string: A string containing the peak list data in JSON format.
    :return: A list of peaks, where each peak is represented as a list containing the m/z value and intensity.
    :rtype: list[list[float, float]]
    """
    # A regular expression pattern is used here to find all matches within the peak list string. Each match
    # is a peak represented in JSON format.
    peaks = re.findall(scripts.globals_vars.peak_list_json_pattern, peak_list_string)

    # Here, we convert each peak (m/z and intensity) into float type and store them in a list.
    # The list of these peaks is returned by the function.
    return [[float(mz), float(intensity)] for mz, intensity in peaks]

def parsing_MoNA_json(json_dict):
    """
    :convert_MoNA_json:

    Converts a JSON dictionary representing a MoNA (MassBank of North America) entry into a formatted dictionary.

    :param json_dict: A dictionary representing a MoNA entry in JSON format.
    :return: A dictionary containing the converted information from the MoNA entry.

    """
    dict_final = {}  # Initialize the final output dictionary

    # Try to fetch compound name from the json_dict, if not available add an empty string
    try:
        dict_final["compound_name"] = json_dict["compound"][0]["names"][0]["name"]
    except:
        dict_final["compound_name"] = ''

    # Try to fetch molecular formula, SMILES, InChI, InChIKey from the json_dict, if not available pass
    try:
        for i in range(len(json_dict["compound"][0]["metaData"])):
            if json_dict["compound"][0]["metaData"][i]["name"].lower() in ["molecular formula", "smiles", "inchi", "inchikey"] and not json_dict["compound"][0]["metaData"][i]["computed"]:
                dict_final[json_dict["compound"][0]["metaData"][i]["name"].lower()] = json_dict["compound"][0]["metaData"][i]["value"]
    except:
        pass

    # Try to fetch spectrum_id from the json_dict, if not available pass
    try:
        dict_final["spectrum_id"] = json_dict["id"]
    except:
        pass

    # Try to fetch different metadata from the json_dict, if not available pass
    try:
        for i in range(len(json_dict["metaData"])):
            if json_dict["metaData"][i]["name"] in ["instrument", "instrument type", "ms level", "ionization", "retention time", "ionization mode", "precursor type", "collision energy", "precursor m/z"]:
                dict_final[json_dict["metaData"][i]["name"].lower()] = json_dict["metaData"][i]["value"]
    except:
        pass

    # Try to fetch filename from the json_dict, if not available pass
    try:
        dict_final["filename"] = json_dict["filename"]
    except:
        pass

    # Try to fetch peaks from the json_dict and parse them, if not available pass
    try:
        peak_list_string = json_dict["spectrum"]
        dict_final["peaks"] = parse_MoNA_peak_list(peak_list_string)
    except:
        pass

    # Try to fetch predicted value
    try:
        dict_final["predicted"] = 'false'

        tags = json_dict.get('tags')
        for tag in tags:
            if tag['text'] == 'In-Silico':
                dict_final["predicted"] = 'true'
    except:
        pass


    return dict_final

def parse_others_json_peak_list(peak_list):
    """
    Parses a JSON peak list and returns a list of peaks.

    This function expects a peak list in JSON format. In the first step, it uses a predefined regular expression
    pattern `peak_list_json_to_json_pattern` (which is not shown in this code) to find all peaks in the JSON data.
    The `re.findall` function returns a list of tuples where each tuple contains the mz and the intensity values
    as strings. Next, it loops over the list using a list comprehension. For each tuple, it converts the mz and the
    intensity values to float and groups them as a list. The function returns a list of these list groups. Each list
    group represents a peak with the mz and intensity values.

    :param peak_list: A JSON string representing the peak list.
    :return: A list of peak tuples where each tuple contains the mz (float) and intensity (float) values.
    """

    # Use a regex pattern to find all peaks in the JSON string
    peak_list = re.findall(scripts.globals_vars.peak_list_json_pattern, peak_list)

    # convert each peak's mz and intensity values to float and group them in a list
    # return a list of these peak lists
    return [[float(mz), float(intensity)] for mz, intensity in peak_list]

def json_to_dict(json_dict):
    """
    :param json_dict: A dictionary representing a JSON object.
    :return: The converted JSON object.
    Check if the JSON object represented by the dictionary contains all the required keys.
    If it does, transform it by calling 'convert_MoNA_json' and 'convert_keys'.
    If any peak list keys are present, transform the corresponding values and return the modified object.
    If none of these conditions are met, return None.
    """

    # Define keys needed for conversion
    keys_to_check = ["compound", "id", "metaData", "spectrum", "filename"]

    # Define possible keys for peak values
    peak_list_keys = ["spectrum", "peaks_json", "peaks"]

    # If the JSON dictionary has all keys_to_check,
    # Convert it using convert_MoNA_json function and convert_keys
    if all(key in json_dict for key in keys_to_check):
        json_dict = parsing_MoNA_json(json_dict)  # Transform JSON object
        json_dict = convert_keys(json_dict)  # Standardize keys
        return json_dict

    # Else if it has a peak from peak_list_keys,
    # parse the peak values and convert keys
    else:
        for key in peak_list_keys:
            if key in json_dict:
                json_dict[key] = parse_others_json_peak_list(str(json_dict[key]))  # Transform peak values
                json_dict = convert_keys(json_dict)  # Standardize keys
                return json_dict

    # If there's neither required keys nor peak keys, return None
    return None

def json_to_dict_processing(FINAL_JSON, progress_callback=None, total_items_callback=None, prefix_callback=None,
                            item_type_callback=None):
    """
    Converts a list of JSON objects into processed JSON data inplace, with progress reporting via callbacks.
    :param FINAL_JSON: A list of JSON objects to be processed.
    :param progress_callback: A function to update the progress (optional).
    :param total_items_callback: A function to set the total number of items (optional).
    :param prefix_callback: A function to dynamically set the prefix for the operation (optional).
    :param item_type_callback: A function to specify the type of items processed (optional).
    :return: None
    """
    start = 0
    end = len(FINAL_JSON)

    # Setting total items via callback if provided
    if total_items_callback:
        total_items_callback(end, 0)  # total = end, completed = 0

    # Updating the dynamic prefix via callback if provided
    if prefix_callback:
        prefix_callback("Parsing JSON spectrums:")

    # Specifying the type of items being processed via callback if provided
    if item_type_callback:
        item_type_callback("spectra")

    # Variable to track progress
    processed_items = 0

    # Calculate chunk size once
    chunk_size = calculate_maximized_chunk_size(data_list=FINAL_JSON)

    # Process the data in chunks
    while start < end:
        # Use ThreadPoolExecutor to process the chunk
        with concurrent.futures.ThreadPoolExecutor() as executor:
            FINAL_JSON[start:start + chunk_size] = list(
                executor.map(json_to_dict, FINAL_JSON[start:start + chunk_size]))

        # Filter out None results
        FINAL_JSON[start:start + chunk_size] = [
            item for item in FINAL_JSON[start:start + chunk_size] if item is not None
        ]

        # Update progress
        processed_items += min(chunk_size, end - start)  # Avoid exceeding total size
        if progress_callback:
            progress_callback(processed_items)

        # Move to the next chunk
        start += chunk_size

    return FINAL_JSON


