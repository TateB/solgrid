// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import  {IERC20}  from  "@openzeppelin/contracts/token/ERC20/IERC20.sol" ;
import{Ownable}from"@openzeppelin/contracts/access/Ownable.sol";
import   {   SafeERC20   }   from   "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol"   ;

contract Imports is Ownable {
    using SafeERC20 for IERC20;

    constructor()   Ownable(msg.sender)   {}

    function   transfer(IERC20 token, address to, uint256 amount) external onlyOwner {
        token.safeTransfer(to, amount);
    }
}
